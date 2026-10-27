//! Collision-aware graph label placement shared by vector and raster renderers.
//!
//! Renderers supply the dimensions of the text they actually paint. Placement is resolved in
//! paint order so an opaque label plate cannot hide an earlier label, node, or foreign route.

use crate::{
    DiagramLayout, ErMarkerGlyph, LayoutEdgePath, LayoutPoint, LayoutRect, MarkerKind,
    edge_label_position,
};
use fm_core::{ArrowType, DiagramType, IrStyleTarget, MermaidDiagramIr};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Diagrams whose labels annotate routed graph edges rather than a specialized chart axis.
#[must_use]
pub fn uses_graph_edge_labels(diagram_type: DiagramType) -> bool {
    matches!(
        diagram_type,
        DiagramType::Flowchart
            | DiagramType::State
            | DiagramType::Class
            | DiagramType::Er
            | DiagramType::Requirement
    )
}

/// Resolve the same edge style cascade for both measurement and painting.
#[must_use]
pub fn edge_label_style(ir: &MermaidDiagramIr, edge_index: usize) -> BTreeMap<String, String> {
    let mut properties = BTreeMap::new();
    for target in [IrStyleTarget::LinkDefault, IrStyleTarget::Link(edge_index)] {
        for style in &ir.style_refs {
            if style.target == target {
                properties.extend(fm_core::parse_style_string(&style.style).properties);
            }
        }
    }
    if let Some(style) = ir
        .edges
        .get(edge_index)
        .and_then(|edge| edge.inline_style.as_ref())
    {
        properties.extend(style.properties.clone());
    }
    properties
}

/// Caption footprints, not whole cluster rectangles: an internal transition may use the
/// container's empty space, but its opaque label must never erase the container's heading.
fn graph_cluster_caption_bounds(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    metrics: &fm_core::FontMetrics,
) -> Vec<LayoutRect> {
    let mut captions = Vec::new();
    let base_font_size = (metrics.font_size() * crate::CLUSTER_FONT_SCALE).max(10.0);
    let baseline = (base_font_size + 2.0).max(16.0);
    for cluster in &layout.clusters {
        let metadata = ir.clusters.get(cluster.cluster_index);
        let title = cluster
            .title
            .as_deref()
            .or_else(|| {
                metadata
                    .and_then(|cluster| cluster.title)
                    .and_then(|label| ir.labels.get(label.0))
                    .map(|label| label.text.as_str())
            })
            .unwrap_or_default();
        let mut declared_font_size = None;
        for style in &ir.style_refs {
            if style.target == IrStyleTarget::Cluster(cluster.cluster_index)
                && let Some(size) = fm_core::parse_style_string(&style.style)
                    .properties
                    .get("font-size")
                    .and_then(|size| size.trim().trim_end_matches("px").parse::<f32>().ok())
                    .filter(|size| size.is_finite() && *size > 0.0 && *size <= 512.0)
            {
                declared_font_size = Some(size);
            }
        }
        let mut include = |text: &str, font_size: f32, svg_baseline: f32, canvas_top: f32| {
            let displayed = crate::displayed_cluster_title(text);
            if displayed.trim().is_empty() {
                return;
            }
            // Reuse the same proportional/wide-glyph allowance that sizes the frame. That
            // helper scales a node font to the cluster face; undo the scale for an authored
            // caption font. The shared scene's 12px face must fit even with a smaller preset.
            let caption_metrics = fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
                font_size: font_size.max(12.0) / crate::CLUSTER_FONT_SCALE,
                ..metrics.config().clone()
            });
            let additional_lines = displayed.lines().count().saturating_sub(1) as f32;
            let top = (svg_baseline - font_size).min(canvas_top).min(8.0);
            let bottom = (svg_baseline + additional_lines * font_size * 1.4 + font_size * 0.3)
                .max(canvas_top + (additional_lines + 1.0) * base_font_size * 1.2)
                .max(8.0 + (additional_lines + 1.0) * 12.0 * 1.2);
            captions.push(LayoutRect {
                x: cluster.bounds.x + 8.0,
                y: cluster.bounds.y + top,
                // Shared scene captions start two pixels farther right than SVG/Canvas.
                width: crate::cluster_title_width(displayed, &caption_metrics) - 16.0 + 2.0,
                height: bottom - top,
            });
        };
        include(
            title,
            declared_font_size.unwrap_or(base_font_size),
            baseline,
            4.0,
        );
        if let Some(kind) = metadata
            .and_then(|cluster| cluster.c4_boundary_type.as_deref())
            .filter(|kind| !kind.is_empty())
        {
            include(
                &format!("[{kind}]"),
                base_font_size,
                baseline + base_font_size * 1.25,
                4.0 + metrics.font_size() * 1.25,
            );
        }
    }
    captions
}

/// Stateful placement for one rendered graph, with shared route and previous-label obstacles.
pub struct EdgeLabelPlacer<'a> {
    ir: &'a MermaidDiagramIr,
    layout: &'a DiagramLayout,
    padding: f32,
    routes: Vec<(usize, EdgeLabelObstacle)>,
    captions: Vec<LayoutRect>,
    metrics: fm_core::FontMetrics,
    plates: Vec<LayoutRect>,
    containers: Arc<Vec<Vec<usize>>>,
    stroke_widths: Arc<Vec<f32>>,
}

impl<'a> EdgeLabelPlacer<'a> {
    /// Prepare obstacles once. Call [`Self::place`] in the renderer's label paint order.
    #[must_use]
    pub fn new(ir: &'a MermaidDiagramIr, layout: &'a DiagramLayout, padding: f32) -> Self {
        Self::with_font_metrics(
            ir,
            layout,
            padding,
            &fm_core::FontMetrics::default_metrics(),
        )
    }

    /// Use the surface's font metrics for subgraph captions as well as the measured edge plates.
    /// A cluster's interior is usable routing space; only its painted caption is an obstacle.
    #[must_use]
    pub fn with_font_metrics(
        ir: &'a MermaidDiagramIr,
        layout: &'a DiagramLayout,
        padding: f32,
        metrics: &fm_core::FontMetrics,
    ) -> Self {
        Self {
            ir,
            layout,
            padding,
            routes: graph_edge_label_route_obstacles(ir, layout),
            captions: graph_cluster_caption_bounds(ir, layout, metrics),
            metrics: metrics.clone(),
            plates: Vec::new(),
            containers: Arc::new(crate::composite_containers(ir, ir.nodes.len())),
            stroke_widths: Arc::new(
                ir.edges
                    .iter()
                    .enumerate()
                    .map(|(index, edge)| {
                        let default = if matches!(
                            edge.arrow,
                            ArrowType::ThickArrow
                                | ArrowType::DoubleThickArrow
                                | ArrowType::ThickLine
                        ) {
                            2.5
                        } else {
                            1.5
                        };
                        edge_label_style(ir, index)
                            .get("stroke-width")
                            .and_then(|width| width.trim_end_matches("px").parse::<f32>().ok())
                            .filter(|width| width.is_finite() && *width > 0.0)
                            .unwrap_or(default)
                    })
                    .collect(),
            ),
        }
    }

    /// Measure one disconnected component without rescanning the rest of the diagram for every
    /// label. Its layout retains the original node/edge indexes, so semantic container and style
    /// lookups can be shared unchanged. Routes and previous plates stay local to this component.
    pub(crate) fn for_component<'b>(&'b self, layout: &'b DiagramLayout) -> EdgeLabelPlacer<'b> {
        EdgeLabelPlacer {
            ir: self.ir,
            layout,
            padding: self.padding,
            routes: graph_edge_label_route_obstacles(self.ir, layout),
            captions: graph_cluster_caption_bounds(self.ir, layout, &self.metrics),
            metrics: self.metrics.clone(),
            plates: Vec::new(),
            containers: Arc::clone(&self.containers),
            stroke_widths: Arc::clone(&self.stroke_widths),
        }
    }

    /// Find a clear center for a measured backing plate and reserve it for subsequent labels.
    /// The returned plate can extend outside the original layout; include it in render bounds.
    pub fn place(&mut self, path: &LayoutEdgePath, width: f32, height: f32) -> LayoutPoint {
        let mut foreign_routes = Vec::new();
        for &(edge_index, obstacle) in &self.routes {
            match obstacle {
                EdgeLabelObstacle::Bounds(_) => foreign_routes.push(obstacle),
                EdgeLabelObstacle::Segment(from, to) if edge_index != path.edge_index => {
                    let tolerance = (self.stroke_widths.get(edge_index).copied().unwrap_or(1.5)
                        + self
                            .stroke_widths
                            .get(path.edge_index)
                            .copied()
                            .unwrap_or(1.5))
                        / 2.0;
                    append_foreign_route_obstacles_with_tolerance(
                        path,
                        from,
                        to,
                        tolerance,
                        &mut foreign_routes,
                    );
                }
                EdgeLabelObstacle::Segment(_, _) => {}
            }
        }
        foreign_routes.extend(self.captions.iter().copied().map(EdgeLabelObstacle::Bounds));
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
        let obstacles: Vec<_> = self
            .layout
            .nodes
            .iter()
            .filter(|node| !ignored_nodes.contains(&node.node_index))
            .map(|node| EdgeLabelObstacle::Bounds(node.bounds))
            .chain(
                self.layout
                    .extensions
                    .state_notes
                    .iter()
                    .map(|note| EdgeLabelObstacle::Bounds(note.bounds)),
            )
            .chain(self.plates.iter().copied().map(EdgeLabelObstacle::Bounds))
            .chain(foreign_routes.iter().copied())
            .collect();
        let center = closest_clear_route_position(
            path,
            edge_label_position(path),
            width,
            height,
            &obstacles,
        )
        .or_else(|| {
            closest_clear_touching_route_position(
                path,
                edge_label_position(path),
                width,
                height,
                &obstacles,
                self.layout.bounds,
            )
        })
        .unwrap_or_else(|| {
            closest_clear_route_neighborhood(
                path,
                width,
                height,
                self.layout,
                self.padding,
                &self.plates,
                &foreign_routes,
                &ignored_nodes,
            )
        });
        self.plates.push(LayoutRect {
            x: center.x - width / 2.0,
            y: center.y - height / 2.0,
            width,
            height,
        });
        center
    }

    /// Replace the latest reservation after a raster surface snaps it to its actual cells.
    /// Pass `None` when the finite viewport cannot paint this label.
    pub fn update_last_placement(&mut self, bounds: Option<LayoutRect>) {
        if let Some(last) = self.plates.last_mut() {
            if let Some(bounds) = bounds {
                *last = bounds;
            } else {
                self.plates.pop();
            }
        }
    }

    /// Original layout plus every measured label plate, for camera fitting and export clipping.
    #[must_use]
    pub fn bounds(&self) -> LayoutRect {
        self.plates
            .iter()
            .fold(self.layout.bounds, |bounds, plate| {
                let x = bounds.x.min(plate.x);
                let y = bounds.y.min(plate.y);
                LayoutRect {
                    x,
                    y,
                    width: (bounds.x + bounds.width).max(plate.x + plate.width) - x,
                    height: (bounds.y + bounds.height).max(plate.y + plate.height) - y,
                }
            })
    }
}

/// Retain foreign branches beyond a shared trunk. Exempting an entire segment merely because
/// part of it is shared allows a plate to erase a different edge beyond the branching point.
pub fn append_foreign_route_obstacles(
    path: &LayoutEdgePath,
    from: LayoutPoint,
    to: LayoutPoint,
    obstacles: &mut Vec<EdgeLabelObstacle>,
) {
    append_foreign_route_obstacles_with_tolerance(path, from, to, 0.0, obstacles);
}

fn append_foreign_route_obstacles_with_tolerance(
    path: &LayoutEdgePath,
    from: LayoutPoint,
    to: LayoutPoint,
    tolerance: f32,
    obstacles: &mut Vec<EdgeLabelObstacle>,
) {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f32::EPSILON {
        return;
    }
    let mut shared = Vec::new();
    for pair in path.points.windows(2) {
        if edge_label_routes_share_corridor(from, to, pair[0], pair[1], tolerance) {
            let a = ((pair[0].x - from.x) * dx + (pair[0].y - from.y) * dy) / length_squared;
            let b = ((pair[1].x - from.x) * dx + (pair[1].y - from.y) * dy) / length_squared;
            shared.push((a.min(b).max(0.0), a.max(b).min(1.0)));
        }
    }
    shared.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let point = |t| LayoutPoint {
        x: dx.mul_add(t, from.x),
        y: dy.mul_add(t, from.y),
    };
    let mut cursor = 0.0_f32;
    for (start, end) in shared {
        if start > cursor {
            obstacles.push(EdgeLabelObstacle::Segment(point(cursor), point(start)));
        }
        cursor = cursor.max(end);
    }
    if cursor < 1.0 {
        obstacles.push(EdgeLabelObstacle::Segment(point(cursor), to));
    }
}

/// Parameter interval in which a moving plate intersects one obstacle. A segment's
/// Minkowski sum with the plate has three separating axes, including its normal;
/// a bounding box alone incorrectly blocks empty corners beside diagonal routes.
fn forbidden_route_interval(
    from: LayoutPoint,
    to: LayoutPoint,
    obstacle: EdgeLabelObstacle,
    width: f32,
    height: f32,
) -> EdgeLabelInterval {
    let (half_width, half_height) = (width / 2.0 + 2.0, height / 2.0 + 2.0);
    let (a, b, cx, cy) = match obstacle {
        EdgeLabelObstacle::Bounds(bounds) => (
            LayoutPoint {
                x: bounds.x,
                y: bounds.y,
            },
            LayoutPoint {
                x: bounds.x + bounds.width,
                y: bounds.y + bounds.height,
            },
            half_width,
            half_height,
        ),
        EdgeLabelObstacle::Segment(a, b) => (a, b, half_width + 3.0, half_height + 3.0),
    };
    let mut interval = (0.0_f32, 1.0_f32);
    let mut clip = |origin: f32, delta: f32, lo: f32, hi: f32| {
        if delta.abs() <= f32::EPSILON {
            return lo < origin && origin < hi;
        }
        let first = (lo - origin) / delta;
        let second = (hi - origin) / delta;
        interval.0 = interval.0.max(first.min(second));
        interval.1 = interval.1.min(first.max(second));
        interval.0 < interval.1
    };
    if !clip(from.x, to.x - from.x, a.x.min(b.x) - cx, a.x.max(b.x) + cx)
        || !clip(from.y, to.y - from.y, a.y.min(b.y) - cy, a.y.max(b.y) + cy)
    {
        return None;
    }
    if matches!(obstacle, EdgeLabelObstacle::Segment(_, _)) {
        let normal = (a.y - b.y, b.x - a.x);
        let base = normal.0 * a.x + normal.1 * a.y;
        let radius = normal.0.abs() * cx + normal.1.abs() * cy;
        if !clip(
            normal.0 * from.x + normal.1 * from.y,
            normal.0 * (to.x - from.x) + normal.1 * (to.y - from.y),
            base - radius,
            base + radius,
        ) {
            return None;
        }
    }
    Some(interval)
}

fn closest_clear_route_position(
    path: &LayoutEdgePath,
    anchor: LayoutPoint,
    width: f32,
    height: f32,
    obstacles: &[EdgeLabelObstacle],
) -> Option<LayoutPoint> {
    closest_clear_route_position_with_offset(
        path,
        anchor,
        width,
        height,
        obstacles,
        LayoutPoint { x: 0.0, y: 0.0 },
    )
}

/// Search the complete route for a clear plate whose side touches its connector. A
/// narrow corridor may have no room for a centered plate while still having a readable
/// slot beside it; checking only route vertices and midpoints misses these intervals.
fn closest_clear_touching_route_position(
    path: &LayoutEdgePath,
    anchor: LayoutPoint,
    width: f32,
    height: f32,
    obstacles: &[EdgeLabelObstacle],
    bounds: LayoutRect,
) -> Option<LayoutPoint> {
    let mut best = None;
    let mut best_score = (f32::INFINITY, f32::INFINITY, f32::INFINITY);
    for (x, y) in [
        (width / 2.0, 0.0),
        (-width / 2.0, 0.0),
        (0.0, height / 2.0),
        (0.0, -height / 2.0),
    ] {
        // Keep obstacles and shared-corridor exemptions from the original route.
        // Only the candidate plate center is translated.
        let Some(center) = closest_clear_route_position_with_offset(
            path,
            anchor,
            width,
            height,
            obstacles,
            LayoutPoint { x, y },
        ) else {
            continue;
        };
        let plate = LayoutRect {
            x: center.x - width / 2.0,
            y: center.y - height / 2.0,
            width,
            height,
        };
        let area = ((bounds.x + bounds.width).max(plate.x + width) - bounds.x.min(plate.x))
            * ((bounds.y + bounds.height).max(plate.y + height) - bounds.y.min(plate.y));
        let score = (
            edge_label_route_distance_squared(path, plate),
            area,
            (center.x - anchor.x).powi(2) + (center.y - anchor.y).powi(2),
        );
        if score < best_score {
            best = Some(center);
            best_score = score;
        }
    }
    best
}

fn closest_clear_route_position_with_offset(
    path: &LayoutEdgePath,
    anchor: LayoutPoint,
    width: f32,
    height: f32,
    obstacles: &[EdgeLabelObstacle],
    offset: LayoutPoint,
) -> Option<LayoutPoint> {
    let mut best = None;
    let mut best_distance = f32::INFINITY;
    for pair in path.points.windows(2) {
        let from = LayoutPoint {
            x: pair[0].x + offset.x,
            y: pair[0].y + offset.y,
        };
        let to = LayoutPoint {
            x: pair[1].x + offset.x,
            y: pair[1].y + offset.y,
        };
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let length_squared = dx * dx + dy * dy;
        if length_squared <= f32::EPSILON {
            continue;
        }
        let preferred = (((anchor.x - from.x) * dx + (anchor.y - from.y) * dy) / length_squared)
            .clamp(0.0, 1.0);
        let mut intervals: Vec<_> = obstacles
            .iter()
            .filter_map(|&obstacle| forbidden_route_interval(from, to, obstacle, width, height))
            .collect();
        intervals.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        // Advance a fraction of a pixel past an exclusion boundary so f32 rounding cannot
        // turn a tangency into an overlap when a renderer reconstructs the plate.
        let epsilon = 0.01 / length_squared.sqrt();
        let mut cursor = 0.0_f32;
        let mut consider = |lo: f32, hi: f32| {
            if lo > hi {
                return;
            }
            let t = preferred.clamp(lo, hi);
            let candidate = LayoutPoint {
                x: dx.mul_add(t, from.x),
                y: dy.mul_add(t, from.y),
            };
            let distance = (candidate.x - anchor.x).powi(2) + (candidate.y - anchor.y).powi(2);
            if distance < best_distance {
                best = Some(candidate);
                best_distance = distance;
            }
        };
        for (lo, hi) in intervals {
            consider(cursor, lo - epsilon);
            cursor = cursor.max(hi + epsilon);
        }
        consider(cursor, 1.0);
    }
    best
}

#[allow(clippy::too_many_arguments)]
fn closest_clear_route_neighborhood(
    path: &LayoutEdgePath,
    width: f32,
    height: f32,
    layout: &DiagramLayout,
    padding: f32,
    plates: &[LayoutRect],
    routes: &[EdgeLabelObstacle],
    ignored_nodes: &[usize],
) -> LayoutPoint {
    let anchor = edge_label_position(path);
    let mut best = anchor;
    let mut best_score = (f32::INFINITY, f32::INFINITY, f32::INFINITY);
    for origin in std::iter::once(anchor)
        .chain(path.points.iter().copied())
        .chain(path.points.windows(2).map(|p| LayoutPoint {
            x: f32::midpoint(p[0].x, p[1].x),
            y: f32::midpoint(p[0].y, p[1].y),
        }))
    {
        for (dx, dy) in [
            (0.0, 0.0),
            (width / 2.0, 0.0),
            (-width / 2.0, 0.0),
            (0.0, height / 2.0),
            (0.0, -height / 2.0),
        ] {
            let center = clear_edge_label_position_ignoring_nodes(
                LayoutPoint {
                    x: origin.x + dx,
                    y: origin.y + dy,
                },
                width,
                height,
                layout,
                padding,
                plates,
                routes,
                ignored_nodes,
            );
            let plate = LayoutRect {
                x: center.x - width / 2.0,
                y: center.y - height / 2.0,
                width,
                height,
            };
            let area = ((layout.bounds.x + layout.bounds.width).max(plate.x + width)
                - layout.bounds.x.min(plate.x))
                * ((layout.bounds.y + layout.bounds.height).max(plate.y + height)
                    - layout.bounds.y.min(plate.y));
            let score = (
                edge_label_route_distance_squared(path, plate),
                area,
                (center.x - anchor.x).powi(2) + (center.y - anchor.y).powi(2),
            );
            if score < best_score {
                best = center;
                best_score = score;
            }
        }
    }
    best
}

/// Squared distance from the painted plate to its own connector, zero when they touch.
#[must_use]
pub fn edge_label_route_distance_squared(path: &LayoutEdgePath, plate: LayoutRect) -> f32 {
    let point_distance = |point: LayoutPoint| {
        let dx = (plate.x - point.x)
            .max(0.0)
            .max(point.x - plate.x - plate.width);
        let dy = (plate.y - point.y)
            .max(0.0)
            .max(point.y - plate.y - plate.height);
        dx * dx + dy * dy
    };
    path.points
        .windows(2)
        .map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            // Cancel the collision helper's two-pixel clearance when measuring actual ink.
            if forbidden_route_interval(a, b, EdgeLabelObstacle::Bounds(plate), -4.0, -4.0)
                .is_some()
            {
                return 0.0;
            }
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let length_squared = dx * dx + dy * dy;
            let mut distance = point_distance(a).min(point_distance(b));
            if length_squared > f32::EPSILON {
                for (x, y) in [
                    (plate.x, plate.y),
                    (plate.x + plate.width, plate.y),
                    (plate.x, plate.y + plate.height),
                    (plate.x + plate.width, plate.y + plate.height),
                ] {
                    let t = (((x - a.x) * dx + (y - a.y) * dy) / length_squared).clamp(0.0, 1.0);
                    distance = distance
                        .min((dx.mul_add(t, a.x) - x).powi(2) + (dy.mul_add(t, a.y) - y).powi(2));
                }
            }
            distance
        })
        .fold(f32::INFINITY, f32::min)
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
    edge_label_routes_share_corridor(a, b, from, to, 0.0)
}

fn edge_label_routes_share_corridor(
    a: LayoutPoint,
    b: LayoutPoint,
    from: LayoutPoint,
    to: LayoutPoint,
    tolerance: f32,
) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length_squared = dx * dx + dy * dy;
    let cross_from = (from.x - a.x) * dy - (from.y - a.y) * dx;
    let cross_to = (to.x - a.x) * dy - (to.y - a.y) * dx;
    let along_from = (from.x - a.x) * dx + (from.y - a.y) * dy;
    let along_to = (to.x - a.x) * dx + (to.y - a.y) * dy;
    // Unequal node widths can put opposing parallel trunks less than a stroke apart.
    // Their painted ink already shares a corridor: treating one as a foreign crossing
    // ejects every label from the other. Exempt only the longitudinal overlap, and only
    // when BOTH ends remain inside the two strokes' combined half-widths.
    let cross_tolerance = tolerance * length_squared.sqrt();
    length_squared > f32::EPSILON
        && cross_from.abs() <= cross_tolerance
        && cross_to.abs() <= cross_tolerance
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
    // Consume the shared marker semantics, including reversed paths, UML and co-arrows.
    // Keeping endpoint decoration in the obstacle set also protects a label's OWN arrow.
    let markers: BTreeMap<_, _> = crate::build_edge_layer(ir, layout)
        .children
        .into_iter()
        .filter_map(|item| match item {
            crate::RenderItem::Path(path) => match path.source {
                crate::RenderSource::Edge(index) => {
                    Some((index, (path.marker_start, path.marker_end)))
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
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
        let (start_kind, end_kind) = markers.get(&path.edge_index).copied().unwrap_or_default();
        let style = edge_label_style(ir, path.edge_index);
        let stroke_width = style
            .get("stroke-width")
            .and_then(|value| value.trim_end_matches("px").parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(1.5);
        for (endpoint, neighbor, kind, cardinality) in [
            (
                path.points[0],
                path.points
                    .iter()
                    .copied()
                    .find(|point| *point != path.points[0]),
                start_kind,
                edge.source_cardinality(),
            ),
            (
                *path.points.last().unwrap(),
                path.points
                    .iter()
                    .rev()
                    .copied()
                    .find(|point| Some(point) != path.points.last()),
                end_kind,
                edge.target_cardinality(),
            ),
        ] {
            let Some(neighbor) = neighbor else {
                continue;
            };
            let dx = neighbor.x - endpoint.x;
            let dy = neighbor.y - endpoint.y;
            let length = dx.hypot(dy);
            if length <= f32::EPSILON {
                continue;
            }
            let (dx, dy) = (dx / length, dy / length);
            if ir.diagram_type == DiagramType::Class
                && let Some(text) = cardinality.filter(|text| !text.is_empty())
            {
                let metrics = fm_core::FontMetrics::default_metrics();
                let width = metrics.estimate_multiline_width(text) * 1.15 + 16.0;
                let height = metrics.font_size() * 1.2 + 16.0;
                // SVG offsets to one side of the port; Canvas centres at its inset.
                // The shared envelope contains both actual text footprints.
                obstacles.push((
                    path.edge_index,
                    EdgeLabelObstacle::Bounds(LayoutRect {
                        x: endpoint.x + dx * 16.0 - width / 2.0,
                        y: endpoint.y + dy * 16.0 - height / 2.0,
                        width,
                        height,
                    }),
                ));
            }
            let (depth, half_width) = match kind {
                MarkerKind::None => (4.0, stroke_width / 2.0),
                MarkerKind::ThickArrow => (
                    23.0_f32.max(stroke_width * 8.0),
                    10.0_f32.max(stroke_width * 4.0),
                ),
                MarkerKind::Diamond
                | MarkerKind::DiamondOpen
                | MarkerKind::TriangleOpen
                | MarkerKind::TriangleOpenStart
                | MarkerKind::Lollipop
                | MarkerKind::LollipopStart => (
                    24.0_f32.max(stroke_width * 8.0),
                    8.0_f32.max(stroke_width * 4.0),
                ),
                _ if kind.er_glyph().is_some() => (4.0, stroke_width / 2.0),
                _ => (
                    15.0_f32.max(stroke_width * 8.0),
                    7.0_f32.max(stroke_width * 4.0),
                ),
            };
            let corners = [
                (0.0, -half_width),
                (0.0, half_width),
                (depth, -half_width),
                (depth, half_width),
            ]
            .map(|(along, across)| LayoutPoint {
                x: endpoint.x + dx * along - dy * across,
                y: endpoint.y + dy * along + dx * across,
            });
            let x = corners
                .iter()
                .map(|point| point.x)
                .fold(f32::INFINITY, f32::min);
            let y = corners
                .iter()
                .map(|point| point.y)
                .fold(f32::INFINITY, f32::min);
            obstacles.push((
                path.edge_index,
                EdgeLabelObstacle::Bounds(LayoutRect {
                    x,
                    y,
                    width: corners
                        .iter()
                        .map(|point| point.x)
                        .fold(f32::NEG_INFINITY, f32::max)
                        - x,
                    height: corners
                        .iter()
                        .map(|point| point.y)
                        .fold(f32::NEG_INFINITY, f32::max)
                        - y,
                }),
            ));
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

pub(crate) fn can_refine_graph_label_spacing(ir: &MermaidDiagramIr) -> bool {
    let label_count = ir.edges.iter().filter(|edge| edge.label.is_some()).count();
    uses_graph_edge_labels(ir.diagram_type)
        && ir.nodes.len() <= 64
        && ir.edges.len() <= 128
        && (2..=32).contains(&label_count)
        && ir.constraints.is_empty()
        && ir.graph.nodes.iter().all(|node| node.subgraphs.is_empty())
}

/// Evaluate the final routes with the graphical surfaces' measured plate footprints. The
/// bounded layout refinement uses this same placer; it does not invent a second collision rule.
pub(crate) fn graph_label_attachment_quality(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    base_metrics: &fm_core::FontMetrics,
) -> (usize, f32, f32) {
    let mut detached = 0;
    let mut worst_gap = 0.0_f32;
    let mut area = layout.bounds.width * layout.bounds.height;
    // Canvas measures its installed face, which can have wider narrow letters than the
    // proportional estimate. Score fixed-advance faces as well: a smaller plate does not
    // necessarily reuse the same free interval after earlier labels have been placed.
    for (allowance, minimum_line_height, preset, overhang_em) in [
        (1.125, 0.0, base_metrics.config().preset, 0.0),
        (1.15, 0.0, base_metrics.config().preset, 0.0),
        (
            1.15,
            base_metrics.font_size() * 1.2,
            base_metrics.config().preset,
            0.0,
        ),
        (
            0.95,
            base_metrics.font_size() * 1.2,
            fm_core::FontPreset::Monospace,
            0.0,
        ),
        (
            1.0,
            base_metrics.font_size() * 1.2,
            fm_core::FontPreset::Monospace,
            0.0,
        ),
        // GPU glyph atlas cells extend beyond the proportional advance at either end.
        (
            1.0,
            base_metrics.font_size() * 1.2,
            base_metrics.config().preset,
            1.0,
        ),
    ] {
        let mut placer = EdgeLabelPlacer::with_font_metrics(ir, layout, 20.0, base_metrics);
        for path in &layout.edges {
            let Some(edge) = ir.edges.get(path.edge_index) else {
                continue;
            };
            if path.bundled || path.points.len() < 2 || edge.arrow == ArrowType::Invisible {
                continue;
            }
            let Some(label) = edge.label.and_then(|id| ir.labels.get(id.0)) else {
                continue;
            };
            let style = edge_label_style(ir, path.edge_index);
            let font_size = style
                .get("font-size")
                .and_then(|size| size.trim_end_matches("px").parse::<f32>().ok())
                .filter(|size| size.is_finite() && *size > 0.0 && *size <= 512.0)
                .unwrap_or(base_metrics.font_size() * 0.85);
            let metrics = fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
                preset,
                font_size,
                line_height: 1.2,
                ..base_metrics.config().clone()
            });
            let advance = if overhang_em > 0.0 {
                // Match the GPU atlas, which positions separate glyph cells even when
                // native text shaping would fuse a joined emoji into one glyph.
                label
                    .text
                    .lines()
                    .map(|line| {
                        line.chars()
                            .filter(|glyph| !glyph.is_control())
                            .map(|glyph| {
                                let mut encoded = [0; 4];
                                metrics.estimate_width(glyph.encode_utf8(&mut encoded))
                            })
                            .sum::<f32>()
                    })
                    .fold(0.0_f32, f32::max)
            } else {
                metrics.estimate_multiline_width(&label.text)
            };
            let width = advance * allowance + font_size * overhang_em + 8.0;
            let height = label.text.lines().count().max(1) as f32
                * (font_size * 1.2).max(minimum_line_height)
                + 4.0;
            let center = placer.place(path, width, height);
            let gap = edge_label_route_distance_squared(
                path,
                LayoutRect {
                    x: center.x - width / 2.0,
                    y: center.y - height / 2.0,
                    width,
                    height,
                },
            );
            if gap > 2.25 {
                detached += 1;
                worst_gap = worst_gap.max(gap);
            }
        }
        let bounds = placer.bounds();
        area = area.max(bounds.width * bounds.height);
    }
    (detached, worst_gap, area)
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

    fn route(points: &[(f32, f32)]) -> LayoutEdgePath {
        LayoutEdgePath {
            edge_index: 0,
            span: Span::default(),
            points: points.iter().map(|&(x, y)| LayoutPoint { x, y }).collect(),
            reversed: false,
            is_self_loop: false,
            parallel_offset: 0.0,
            bundle_count: 1,
            bundled: false,
        }
    }

    #[test]
    fn continuous_side_contact_finds_a_slot_between_route_vertices() {
        let path = route(&[(0.0, 50.0), (400.0, 50.0)]);
        let anchor = LayoutPoint { x: 200.0, y: 50.0 };
        let node = LayoutRect {
            x: 120.0,
            y: 10.0,
            width: 190.0,
            height: 35.0,
        };
        let obstacles = [
            EdgeLabelObstacle::Segment(
                LayoutPoint { x: 0.0, y: 60.0 },
                LayoutPoint { x: 400.0, y: 60.0 },
            ),
            EdgeLabelObstacle::Bounds(node),
        ];
        let bounds = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 100.0,
        };
        assert!(closest_clear_route_position(&path, anchor, 80.0, 20.0, &obstacles).is_none());
        let center =
            closest_clear_touching_route_position(&path, anchor, 80.0, 20.0, &obstacles, bounds)
                .expect("a free interval beside the route must be used before detaching the label");
        assert!(center.x > 70.0 && center.x < 80.0);
        assert_eq!(center.y, 40.0);
        let plate = LayoutRect {
            x: center.x - 40.0,
            y: center.y - 10.0,
            width: 80.0,
            height: 20.0,
        };
        assert!(!overlap(plate, node));
        assert!(plate.y + plate.height < 60.0);
        assert!(edge_label_route_distance_squared(&path, plate) < 0.0001);
        assert_eq!(
            Some(center),
            closest_clear_touching_route_position(&path, anchor, 80.0, 20.0, &obstacles, bounds,)
        );
    }

    #[test]
    fn shared_corridor_labels_slide_along_their_connector() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let mut layout = frame();
        layout.bounds.width = 400.0;
        let path = route(&[(10.0, 50.0), (390.0, 50.0)]);
        let mut placer = EdgeLabelPlacer::new(&ir, &layout, 0.0);
        let mut plates: Vec<LayoutRect> = Vec::new();
        for _ in 0..4 {
            let center = placer.place(&path, 72.0, 20.0);
            let plate = LayoutRect {
                x: center.x - 36.0,
                y: center.y - 10.0,
                width: 72.0,
                height: 20.0,
            };
            assert_eq!(center.y, 50.0, "shared labels must remain on their route");
            assert!(plates.iter().all(|previous| !overlap(*previous, plate)));
            plates.push(plate);
        }
        assert_eq!(
            placer.bounds(),
            layout.bounds,
            "available corridor must not add whitespace"
        );
    }

    #[test]
    fn labels_find_free_space_around_a_bend_and_diagonal_crossing() {
        let path = route(&[(0.0, 50.0), (100.0, 50.0), (100.0, 150.0)]);
        let obstacles = [EdgeLabelObstacle::Bounds(LayoutRect {
            x: 0.0,
            y: 30.0,
            width: 120.0,
            height: 40.0,
        })];
        let center = closest_clear_route_position(
            &path,
            LayoutPoint { x: 50.0, y: 50.0 },
            30.0,
            20.0,
            &obstacles,
        )
        .unwrap();
        assert_eq!(center.x, 100.0);
        assert!(center.y >= 82.0);
        let diagonal = [EdgeLabelObstacle::Segment(
            LayoutPoint { x: 0.0, y: 0.0 },
            LayoutPoint { x: 100.0, y: 100.0 },
        )];
        let path = route(&[(0.0, 75.0), (100.0, 75.0)]);
        let center = closest_clear_route_position(
            &path,
            LayoutPoint { x: 10.0, y: 75.0 },
            20.0,
            20.0,
            &diagonal,
        )
        .unwrap();
        assert_eq!(center, LayoutPoint { x: 10.0, y: 75.0 });
    }

    #[test]
    fn a_partial_shared_trunk_keeps_the_foreign_continuation_protected() {
        let path = route(&[(0.0, 50.0), (100.0, 50.0)]);
        let mut obstacles = Vec::new();
        append_foreign_route_obstacles(
            &path,
            LayoutPoint { x: 50.0, y: 50.0 },
            LayoutPoint { x: 150.0, y: 50.0 },
            &mut obstacles,
        );
        assert!(matches!(
            obstacles.as_slice(),
            [EdgeLabelObstacle::Segment(
                LayoutPoint { x: 100.0, y: 50.0 },
                LayoutPoint { x: 150.0, y: 50.0 }
            )]
        ));
    }

    #[test]
    fn painted_shared_corridors_respect_stroke_width_and_keep_distinct_routes() {
        let path = route(&[(0.0, 0.0), (100.0, 0.0)]);
        let mut obstacles = Vec::new();
        append_foreign_route_obstacles_with_tolerance(
            &path,
            LayoutPoint { x: 50.0, y: 0.825 },
            LayoutPoint { x: 150.0, y: 0.825 },
            1.5,
            &mut obstacles,
        );
        assert_eq!(obstacles.len(), 1);
        let EdgeLabelObstacle::Segment(from, to) = obstacles[0] else {
            panic!("expected continuation");
        };
        assert!((from.x - 100.0).abs() < 0.001 && (to.x - 150.0).abs() < 0.001);
        assert!((from.y - 0.825).abs() < 0.001 && (to.y - 0.825).abs() < 0.001);
        for (offset, stroke_width) in [(2.0, 1.5), (0.825, 0.5)] {
            let mut distinct = Vec::new();
            append_foreign_route_obstacles_with_tolerance(
                &path,
                LayoutPoint { x: 0.0, y: offset },
                LayoutPoint {
                    x: 100.0,
                    y: offset,
                },
                stroke_width,
                &mut distinct,
            );
            assert!(
                matches!(distinct.as_slice(), [EdgeLabelObstacle::Segment(from, to)]
                if from.x == 0.0 && to.x == 100.0),
                "distinct ink must remain an obstacle"
            );
        }
        assert!(
            !edge_label_routes_share_corridor(
                LayoutPoint { x: 0.0, y: 0.0 },
                LayoutPoint { x: 100.0, y: 0.0 },
                LayoutPoint { x: 50.0, y: -50.0 },
                LayoutPoint { x: 50.0, y: 50.0 },
                1.5,
            ),
            "a crossing is never a shared trunk"
        );
    }

    #[test]
    fn raster_reservations_replace_continuous_proposals_and_expand_bounds() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let layout = frame();
        let path = route(&[(0.0, 50.0), (100.0, 50.0)]);
        let mut placer = EdgeLabelPlacer::new(&ir, &layout, 0.0);
        placer.place(&path, 40.0, 20.0);
        let snapped = LayoutRect {
            x: 120.0,
            y: 40.0,
            width: 40.0,
            height: 20.0,
        };
        placer.update_last_placement(Some(snapped));
        assert_eq!(placer.bounds().width, 160.0);
        assert_eq!(
            placer.place(&path, 40.0, 20.0),
            LayoutPoint { x: 50.0, y: 50.0 }
        );
        placer.update_last_placement(None);
        placer.update_last_placement(None);
        assert_eq!(placer.bounds(), layout.bounds);
    }

    fn overlap(a: LayoutRect, b: LayoutRect) -> bool {
        a.x < b.x + b.width - 0.01
            && b.x < a.x + a.width - 0.01
            && a.y < b.y + b.height - 0.01
            && b.y < a.y + a.height - 0.01
    }

    #[test]
    fn labels_avoid_cluster_headings_without_excluding_the_container_interior() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let mut layout = frame();
        layout.bounds.width = 400.0;
        layout.bounds.height = 200.0;
        layout.clusters.push(crate::LayoutClusterBox {
            cluster_index: 0,
            span: Span::default(),
            title: Some(String::from("Payment processing")),
            color: None,
            bounds: layout.bounds,
        });
        let caption = LayoutRect {
            x: 8.0,
            y: 4.0,
            width: 120.0,
            height: 18.0,
        };
        let path = route(&[(0.0, 15.0), (200.0, 15.0)]);
        let old_center = edge_label_position(&path);
        assert!(overlap(
            caption,
            LayoutRect {
                x: old_center.x - 20.0,
                y: old_center.y - 10.0,
                width: 40.0,
                height: 20.0,
            }
        ));
        let mut placer = EdgeLabelPlacer::new(&ir, &layout, 0.0);
        let center = placer.place(&path, 40.0, 20.0);
        let plate = LayoutRect {
            x: center.x - 20.0,
            y: center.y - 10.0,
            width: 40.0,
            height: 20.0,
        };
        assert!(!overlap(caption, plate), "edge label obscures the heading");
        assert!(edge_label_route_distance_squared(&path, plate) < 0.01);
        assert_eq!(
            EdgeLabelPlacer::new(&ir, &layout, 0.0).place(&path, 40.0, 20.0),
            center
        );
        let internal_path = route(&[(40.0, 100.0), (160.0, 100.0)]);
        assert_eq!(
            placer.place(&internal_path, 40.0, 20.0),
            LayoutPoint { x: 100.0, y: 100.0 },
            "only the caption blocks labels, not the whole subgraph"
        );
        // Component packing resolves labels against local copies; their captions must not
        // disappear from the obstacle set when the component is isolated for measurement.
        let mut local = placer.for_component(&layout);
        assert_eq!(local.place(&path, 40.0, 20.0), center);
    }

    #[test]
    fn caption_obstacles_survive_the_neighborhood_fallback_and_expand_the_frame() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let mut layout = frame();
        layout.clusters.push(crate::LayoutClusterBox {
            cluster_index: 0,
            span: Span::default(),
            title: Some(String::from("Payment processing\nExternal providers")),
            color: None,
            bounds: layout.bounds,
        });
        let path = route(&[(40.0, 12.0), (60.0, 12.0)]);
        let mut placer = EdgeLabelPlacer::new(&ir, &layout, 0.0);
        let center = placer.place(&path, 80.0, 24.0);
        let plate = LayoutRect {
            x: center.x - 40.0,
            y: center.y - 12.0,
            width: 80.0,
            height: 24.0,
        };
        let ink = LayoutRect {
            x: 8.0,
            y: 4.0,
            width: 120.0,
            height: 34.0,
        };
        assert!(!overlap(ink, plate), "fallback lost the heading obstacle");
        assert!(
            plate.x < 0.0 || plate.y < 0.0,
            "a blocked route must find clear space beyond the original frame"
        );
        let bounds = placer.bounds();
        assert!(bounds.x <= plate.x && bounds.y <= plate.y);
        assert!(bounds.x + bounds.width >= plate.x + plate.width);
        assert!(bounds.y + bounds.height >= plate.y + plate.height);
    }

    #[test]
    fn multiline_and_styled_cluster_captions_use_the_surface_font_metrics() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let mut layout = frame();
        layout.clusters.push(crate::LayoutClusterBox {
            cluster_index: 0,
            span: Span::default(),
            title: Some(String::from("Payments\nRisk review")),
            color: None,
            bounds: layout.bounds,
        });
        let normal = fm_core::FontMetrics::default_metrics();
        let large = fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
            font_size: 30.0,
            ..Default::default()
        });
        let small_bounds = graph_cluster_caption_bounds(&ir, &layout, &normal)[0];
        let large_bounds = graph_cluster_caption_bounds(&ir, &layout, &large)[0];
        assert!(large_bounds.width > small_bounds.width * 1.8);
        assert!(large_bounds.height > small_bounds.height * 1.8);
        ir.style_refs.push(fm_core::IrStyleRef {
            target: IrStyleTarget::Cluster(0),
            style: String::from("font-size:40px"),
            span: Span::default(),
        });
        let styled = graph_cluster_caption_bounds(&ir, &layout, &normal)[0];
        // SVG keeps the caption's baseline at 16px while CSS enlarges the face. Both
        // ascenders above that baseline and the second row below it need protection.
        assert!(styled.y <= -24.0);
        assert!(styled.y + styled.height >= 83.99);
        assert!(styled.width > large_bounds.width);
        ir.clusters.push(fm_core::IrCluster {
            c4_boundary_type: Some(String::from("SYSTEM")),
            ..Default::default()
        });
        let with_type = graph_cluster_caption_bounds(&ir, &layout, &normal);
        assert_eq!(with_type.len(), 2);
        assert!(with_type[1].y + with_type[1].height >= 38.0);
        layout.clusters[0].title = Some(String::from("   "));
        ir.clusters.clear();
        assert!(graph_cluster_caption_bounds(&ir, &layout, &normal).is_empty());
    }

    #[test]
    fn requirement_relationships_receive_collision_free_scene_plates_and_bounds() {
        let source = "requirementDiagram\nrequirement auth {\nid: R1\ntext: Authenticate each request\nrisk: High\nverifymethod: Test\n}\nrequirement audit {\nid: R2\ntext: Keep a complete audit record\nrisk: Medium\nverifymethod: Test\n}\nelement gateway {\ntype: service\n}\ngateway - satisfies -> auth\ngateway - satisfies -> audit\naudit - traces -> auth\n";
        let ir = fm_parser::parse(source).ir;
        assert_eq!(ir.diagram_type, DiagramType::Requirement);
        assert_eq!(ir.edges.len(), 3);
        let layout = crate::layout_diagram(&ir);
        let scene = crate::build_render_scene(&ir, &layout);
        let plates = scene_plates(&scene);
        assert_eq!(
            plates.len(),
            3,
            "requirement captions skipped shared placement"
        );
        for (position, (_, plate)) in plates.iter().enumerate() {
            assert!(
                layout
                    .nodes
                    .iter()
                    .all(|node| !overlap(*plate, node.bounds))
            );
            assert!(
                plates[position + 1..]
                    .iter()
                    .all(|(_, other)| !overlap(*plate, *other))
            );
            assert!(plate.x >= scene.bounds.x - 0.01 && plate.y >= scene.bounds.y - 0.01);
            assert!(plate.x + plate.width <= scene.bounds.x + scene.bounds.width + 0.01);
            assert!(plate.y + plate.height <= scene.bounds.y + scene.bounds.height + 0.01);
        }
        assert_eq!(
            scene,
            crate::build_render_scene(&ir, &crate::layout_diagram(&ir))
        );
    }

    const ISSUE_16: &str = "stateDiagram-v2\n[*] --> Idle\nIdle --> Starting: Space down / tap\nStarting --> Idle: closed\nStarting --> Recording: opened\nStarting --> Releasing: Space up, focus lost\nRecording --> Idle: closed\nRecording --> Stopping: Space up, focus lost / tap\nReleasing --> Idle: closed\nReleasing --> Stopping: opened / tap\nStopping --> Settling: closed\nSettling --> Idle: quiet 1 s\nSettling --> Stopping: opened / tap\n";

    fn scene_plates(scene: &crate::RenderScene) -> Vec<(usize, LayoutRect)> {
        let layer = scene
            .root
            .children
            .iter()
            .find_map(|item| match item {
                crate::RenderItem::Group(group) if group.id.as_deref() == Some("labels") => {
                    Some(group)
                }
                _ => None,
            })
            .unwrap();
        layer
            .children
            .iter()
            .filter_map(|item| {
                let crate::RenderItem::Path(path) = item else {
                    return None;
                };
                let crate::RenderSource::Edge(index) = path.source else {
                    return None;
                };
                let points: Vec<_> = path
                    .commands
                    .iter()
                    .filter_map(|command| match *command {
                        crate::PathCmd::MoveTo { x, y }
                        | crate::PathCmd::LineTo { x, y }
                        | crate::PathCmd::QuadTo { x, y, .. } => Some((x, y)),
                        _ => None,
                    })
                    .collect();
                let x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
                let y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
                Some((
                    index,
                    LayoutRect {
                        x,
                        y,
                        width: points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max) - x,
                        height: points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max) - y,
                    },
                ))
            })
            .collect()
    }

    #[test]
    fn issue_16_scene_labels_are_readable_attached_and_deterministic() {
        for direction in [fm_core::GraphDirection::TB, fm_core::GraphDirection::LR] {
            for routing in [crate::EdgeRouting::Orthogonal, crate::EdgeRouting::Spline] {
                for algorithm in [
                    crate::LayoutAlgorithm::Auto,
                    crate::LayoutAlgorithm::Sugiyama,
                ] {
                    let mut ir = fm_parser::parse(ISSUE_16).ir;
                    ir.direction = direction;
                    let config = crate::LayoutConfig {
                        edge_routing: routing,
                        ..Default::default()
                    };
                    let layout =
                        crate::layout_diagram_traced_with_config(&ir, algorithm, config.clone())
                            .layout;
                    let scene = crate::build_render_scene(&ir, &layout);
                    let repeat =
                        crate::layout_diagram_traced_with_config(&ir, algorithm, config).layout;
                    assert_eq!(layout, repeat);
                    assert_eq!(scene, crate::build_render_scene(&ir, &repeat));
                    let plates = scene_plates(&scene);
                    assert_eq!(layout.edges.len(), 12);
                    assert_eq!(plates.len(), 11);
                    let mut pairs = 0;
                    for (position, (index, plate)) in plates.iter().enumerate() {
                        for (_, other) in &plates[position + 1..] {
                            pairs += 1;
                            assert!(
                                !overlap(*plate, *other),
                                "{algorithm:?} {direction:?} {routing:?}: {plate:?} overlaps {other:?}"
                            );
                        }
                        assert!(
                            layout
                                .nodes
                                .iter()
                                .all(|node| !overlap(*plate, node.bounds)),
                            "label {index} hides a node"
                        );
                        assert!(
                            plate.x >= scene.bounds.x - 0.01 && plate.y >= scene.bounds.y - 0.01
                        );
                        assert!(
                            plate.x + plate.width <= scene.bounds.x + scene.bounds.width + 0.01
                        );
                        assert!(
                            plate.y + plate.height <= scene.bounds.y + scene.bounds.height + 0.01
                        );
                        let path = layout
                            .edges
                            .iter()
                            .find(|path| path.edge_index == *index)
                            .unwrap();
                        let gap = edge_label_route_distance_squared(path, *plate).sqrt();
                        assert!(
                            gap <= 2.0,
                            "{algorithm:?} {direction:?} {routing:?} label {index} detached by {gap}px"
                        );
                    }
                    assert_eq!(pairs, 55);
                    assert!(
                        scene.bounds.width * scene.bounds.height < 450_000.0,
                        "readability must not require excessive whitespace"
                    );
                }
            }
        }
    }

    #[test]
    fn scene_uses_authored_font_multiline_bounds_and_skips_invisible_labels() {
        let mut ir=fm_parser::parse("flowchart LR\nA -->|label| B\nA -->|hidden| C\nlinkStyle 0 font-size:24px,color:#663399\n").ir;
        let label = ir.edges[0].label.unwrap();
        ir.labels[label.0].text = String::from("a long first line\nsecond row");
        ir.edges[1].arrow = ArrowType::Invisible;
        let layout = crate::layout_diagram(&ir);
        let scene = crate::build_render_scene(&ir, &layout);
        let plates = scene_plates(&scene);
        assert_eq!(plates.len(), 1);
        assert!(plates[0].1.height >= 61.5);
        let label_layer = scene
            .root
            .children
            .iter()
            .find_map(|item| match item {
                crate::RenderItem::Group(group) if group.id.as_deref() == Some("labels") => {
                    Some(group)
                }
                _ => None,
            })
            .unwrap();
        let rows: Vec<_> = label_layer
            .children
            .iter()
            .filter_map(|item| match item {
                crate::RenderItem::Text(text)
                    if matches!(text.source, crate::RenderSource::Edge(_)) =>
                {
                    Some(text)
                }
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|text| text.font_size == 24.0
            && text.fill
                == crate::FillStyle::Solid {
                    color: String::from("#663399"),
                    opacity: 1.0
                }));
        assert!((rows[1].y - rows[0].y - 28.8).abs() < 0.01);
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
