#![forbid(unsafe_code)]

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use fm_core::{GraphDirection, IrEndpoint, MermaidDiagramIr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutAlgorithm {
    Auto,
    Sugiyama,
    Force,
    Tree,
    Radial,
    Timeline,
    Gantt,
    Sankey,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CycleStrategy {
    #[default]
    Greedy,
    DfsBack,
    MfasApprox,
    CycleAware,
}

impl CycleStrategy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Greedy => "greedy",
            Self::DfsBack => "dfs-back",
            Self::MfasApprox => "mfas",
            Self::CycleAware => "cycle-aware",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "greedy" => Some(Self::Greedy),
            "dfs-back" | "dfs_back" | "dfs" => Some(Self::DfsBack),
            "mfas" | "minimum-feedback-arc-set" | "minimum_feedback_arc_set" => {
                Some(Self::MfasApprox)
            }
            "cycle-aware" | "cycle_aware" | "cycleaware" => Some(Self::CycleAware),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LayoutConfig {
    pub cycle_strategy: CycleStrategy,
    pub collapse_cycle_clusters: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LayoutStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub crossing_count: usize,
    /// Crossing count after barycenter (before transpose/sifting refinement).
    pub crossing_count_before_refinement: usize,
    pub reversed_edges: usize,
    pub cycle_count: usize,
    pub cycle_node_count: usize,
    pub max_cycle_size: usize,
    pub collapsed_clusters: usize,
    /// Sum of Euclidean edge lengths for reversed (cycle-breaking) edges.
    pub reversed_edge_total_length: f32,
    /// Sum of Euclidean edge lengths for all edges.
    pub total_edge_length: f32,
    pub phase_iterations: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    #[must_use]
    pub fn center(self) -> LayoutPoint {
        LayoutPoint {
            x: self.x + (self.width / 2.0),
            y: self.y + (self.height / 2.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNodeBox {
    pub node_index: usize,
    pub node_id: String,
    pub rank: usize,
    pub order: usize,
    pub bounds: LayoutRect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutClusterBox {
    pub cluster_index: usize,
    pub bounds: LayoutRect,
}

/// Edge routing style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeRouting {
    /// Manhattan-style orthogonal routing (default).
    #[default]
    Orthogonal,
    /// Cubic Bezier spline routing.
    Spline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutEdgePath {
    pub edge_index: usize,
    pub points: Vec<LayoutPoint>,
    pub reversed: bool,
    /// True if this is a self-loop edge (source == target).
    pub is_self_loop: bool,
    /// Offset for parallel edges (0 for first edge, increments for duplicates).
    pub parallel_offset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutSpacing {
    pub node_spacing: f32,
    pub rank_spacing: f32,
    pub cluster_padding: f32,
}

impl Default for LayoutSpacing {
    fn default() -> Self {
        Self {
            node_spacing: 48.0,
            rank_spacing: 72.0,
            cluster_padding: 24.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutStageSnapshot {
    pub stage: &'static str,
    pub reversed_edges: usize,
    pub crossing_count: usize,
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutTrace {
    pub snapshots: Vec<LayoutStageSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutCycleCluster {
    pub head_node_index: usize,
    pub member_node_indexes: Vec<usize>,
    pub bounds: LayoutRect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagramLayout {
    pub nodes: Vec<LayoutNodeBox>,
    pub clusters: Vec<LayoutClusterBox>,
    pub cycle_clusters: Vec<LayoutCycleCluster>,
    pub edges: Vec<LayoutEdgePath>,
    pub bounds: LayoutRect,
    pub stats: LayoutStats,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracedLayout {
    pub layout: DiagramLayout,
    pub trace: LayoutTrace,
}

#[must_use]
pub fn layout(ir: &MermaidDiagramIr, algorithm: LayoutAlgorithm) -> LayoutStats {
    match algorithm {
        LayoutAlgorithm::Force => layout_diagram_force(ir).stats,
        _ => layout_diagram(ir).stats,
    }
}

#[must_use]
pub fn layout_diagram(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_traced(ir).layout
}

#[must_use]
pub fn layout_diagram_with_cycle_strategy(
    ir: &MermaidDiagramIr,
    cycle_strategy: CycleStrategy,
) -> DiagramLayout {
    layout_diagram_traced_with_cycle_strategy(ir, cycle_strategy).layout
}

#[must_use]
pub fn layout_diagram_with_config(ir: &MermaidDiagramIr, config: LayoutConfig) -> DiagramLayout {
    layout_diagram_traced_with_config(ir, config).layout
}

#[must_use]
pub fn layout_diagram_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    layout_diagram_traced_with_cycle_strategy(ir, default_cycle_strategy())
}

#[must_use]
pub fn layout_diagram_traced_with_cycle_strategy(
    ir: &MermaidDiagramIr,
    cycle_strategy: CycleStrategy,
) -> TracedLayout {
    layout_diagram_traced_with_config(
        ir,
        LayoutConfig {
            cycle_strategy,
            collapse_cycle_clusters: false,
        },
    )
}

#[must_use]
pub fn layout_diagram_traced_with_config(
    ir: &MermaidDiagramIr,
    config: LayoutConfig,
) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let spacing = LayoutSpacing::default();
    let node_sizes = compute_node_sizes(ir);
    let cycle_result = cycle_removal(ir, config.cycle_strategy);
    push_snapshot(
        &mut trace,
        "cycle_removal",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        0,
    );

    let collapse_map = if config.collapse_cycle_clusters {
        Some(build_cycle_cluster_map(ir, &cycle_result))
    } else {
        None
    };

    let ranks = rank_assignment(ir, &cycle_result);
    push_snapshot(
        &mut trace,
        "rank_assignment",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        0,
    );

    let (crossing_count_before, ordering_by_rank) = crossing_minimization(ir, &ranks);
    push_snapshot(
        &mut trace,
        "crossing_minimization",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        crossing_count_before,
    );

    // Refinement: transpose + sifting heuristics.
    let (crossing_count, ordering_by_rank) =
        crossing_refinement(ir, &ranks, ordering_by_rank, crossing_count_before);
    push_snapshot(
        &mut trace,
        "crossing_refinement",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        crossing_count,
    );

    let mut nodes = coordinate_assignment(ir, &node_sizes, &ranks, &ordering_by_rank, spacing);
    let edges = build_edge_paths(ir, &nodes, &cycle_result.highlighted_edge_indexes);
    let mut clusters = build_cluster_boxes(ir, &nodes, spacing);
    let mut cycle_clusters = Vec::new();

    // If cycle clusters are collapsed, group member nodes within their cluster head's bounds.
    let collapsed_count = if let Some(ref collapse_map) = collapse_map {
        let count = collapse_map.cluster_heads.len();
        cycle_clusters =
            build_cycle_cluster_results(collapse_map, &mut nodes, &mut clusters, spacing);
        count
    } else {
        0
    };

    let bounds = compute_bounds(&nodes, &clusters, spacing);

    push_snapshot(
        &mut trace,
        "post_processing",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        crossing_count,
    );

    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);

    let stats = LayoutStats {
        node_count: ir.nodes.len(),
        edge_count: ir.edges.len(),
        crossing_count,
        crossing_count_before_refinement: crossing_count_before,
        reversed_edges: cycle_result.reversed_edge_indexes.len(),
        cycle_count: cycle_result.summary.cycle_count,
        cycle_node_count: cycle_result.summary.cycle_node_count,
        max_cycle_size: cycle_result.summary.max_cycle_size,
        collapsed_clusters: collapsed_count,
        reversed_edge_total_length,
        total_edge_length,
        phase_iterations: trace.snapshots.len(),
    };

    TracedLayout {
        layout: DiagramLayout {
            nodes,
            clusters,
            cycle_clusters,
            edges,
            bounds,
            stats,
        },
        trace,
    }
}

/// Lay out a diagram using force-directed (Fruchterman-Reingold) algorithm.
///
/// Suitable for diagrams without a natural hierarchy: ER diagrams, architecture
/// diagrams, generic graphs with no clear flow direction.
#[must_use]
pub fn layout_diagram_force(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_force_traced(ir).layout
}

/// Lay out with force-directed algorithm and return tracing information.
#[must_use]
pub fn layout_diagram_force_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let spacing = LayoutSpacing::default();
    let node_sizes = compute_node_sizes(ir);
    let n = ir.nodes.len();

    if n == 0 {
        return TracedLayout {
            layout: DiagramLayout {
                nodes: vec![],
                clusters: vec![],
                cycle_clusters: vec![],
                edges: vec![],
                bounds: LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
                stats: LayoutStats::default(),
            },
            trace,
        };
    }

    // Deterministic initial placement using hash of node IDs.
    let mut positions = force_initial_positions(ir, &node_sizes, &spacing);

    push_snapshot(&mut trace, "force_init", n, ir.edges.len(), 0, 0);

    // Build adjacency list for attractive forces.
    let adjacency = force_build_adjacency(ir);

    // Build cluster membership for cluster-aware forces.
    let cluster_membership = force_cluster_membership(ir);

    // Fruchterman-Reingold iterations.
    let area = (n as f32) * spacing.node_spacing * spacing.rank_spacing;
    let k = (area / n as f32).sqrt(); // Optimal distance between nodes
    let max_iterations = force_iteration_budget(n);
    let convergence_threshold = 0.5;

    for iteration in 0..max_iterations {
        let temperature = force_temperature(iteration, max_iterations, k);
        if temperature < convergence_threshold {
            break;
        }

        let displacements = force_compute_displacements(
            &positions,
            &node_sizes,
            &adjacency,
            &cluster_membership,
            k,
            n,
        );

        // Apply displacements clamped by temperature.
        let mut max_displacement: f32 = 0.0;
        for i in 0..n {
            let (dx, dy) = displacements[i];
            let magnitude = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
            let clamped_mag = magnitude.min(temperature);
            let scale = clamped_mag / magnitude;
            positions[i].0 += dx * scale;
            positions[i].1 += dy * scale;
            max_displacement = max_displacement.max(clamped_mag);
        }

        if max_displacement < convergence_threshold {
            break;
        }
    }

    push_snapshot(&mut trace, "force_simulation", n, ir.edges.len(), 0, 0);

    // Overlap removal post-processing.
    force_remove_overlaps(&mut positions, &node_sizes, &spacing);

    push_snapshot(&mut trace, "force_overlap_removal", n, ir.edges.len(), 0, 0);

    // Normalize positions so all coordinates are non-negative.
    force_normalize_positions(&mut positions, &node_sizes);

    // Build layout output.
    let nodes = force_build_node_boxes(ir, &positions, &node_sizes);
    let edges = force_build_edge_paths(ir, &nodes);
    let clusters = build_cluster_boxes(ir, &nodes, spacing);
    let bounds = compute_bounds(&nodes, &clusters, spacing);

    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);

    push_snapshot(&mut trace, "force_post_processing", n, ir.edges.len(), 0, 0);

    let stats = LayoutStats {
        node_count: n,
        edge_count: ir.edges.len(),
        crossing_count: 0, // Not computed for force-directed
        crossing_count_before_refinement: 0,
        reversed_edges: 0,
        cycle_count: 0,
        cycle_node_count: 0,
        max_cycle_size: 0,
        collapsed_clusters: 0,
        reversed_edge_total_length,
        total_edge_length,
        phase_iterations: trace.snapshots.len(),
    };

    TracedLayout {
        layout: DiagramLayout {
            nodes,
            clusters,
            cycle_clusters: vec![],
            edges,
            bounds,
            stats,
        },
        trace,
    }
}

/// Deterministic initial placement using a hash of node IDs.
///
/// Places nodes in a grid pattern with positions offset by a deterministic
/// hash so that the layout doesn't depend on node insertion order.
fn force_initial_positions(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    spacing: &LayoutSpacing,
) -> Vec<(f32, f32)> {
    let n = ir.nodes.len();
    let cols = (n as f32).sqrt().ceil() as usize;
    let cell_size = spacing.node_spacing + spacing.rank_spacing;

    ir.nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            // Deterministic hash: FNV-1a on node ID bytes.
            let hash = fnv1a_hash(node.id.as_bytes());
            // Small perturbation from hash to break symmetry.
            let jitter_x = ((hash & 0xFF) as f32 / 255.0 - 0.5) * cell_size * 0.3;
            let jitter_y = (((hash >> 8) & 0xFF) as f32 / 255.0 - 0.5) * cell_size * 0.3;

            let col = i % cols;
            let row = i / cols;
            let (w, h) = node_sizes[i];
            let x = col as f32 * cell_size + jitter_x + w / 2.0;
            let y = row as f32 * cell_size + jitter_y + h / 2.0;
            (x, y)
        })
        .collect()
}

/// Simple FNV-1a hash for deterministic node placement.
fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Build adjacency list from edges.
fn force_build_adjacency(ir: &MermaidDiagramIr) -> Vec<Vec<usize>> {
    let n = ir.nodes.len();
    let mut adj = vec![Vec::new(); n];
    for edge in &ir.edges {
        let from = endpoint_node_index(ir, edge.from);
        let to = endpoint_node_index(ir, edge.to);
        if let (Some(f), Some(t)) = (from, to)
            && f != t
            && f < n
            && t < n
        {
            adj[f].push(t);
            adj[t].push(f);
        }
    }
    // Deduplicate.
    for neighbors in &mut adj {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    adj
}

/// Map each node to its cluster index (if any).
fn force_cluster_membership(ir: &MermaidDiagramIr) -> Vec<Option<usize>> {
    let n = ir.nodes.len();
    let mut membership = vec![None; n];
    for (ci, cluster) in ir.clusters.iter().enumerate() {
        for member in &cluster.members {
            if member.0 < n {
                membership[member.0] = Some(ci);
            }
        }
    }
    membership
}

/// Compute iteration budget based on graph size.
fn force_iteration_budget(n: usize) -> usize {
    // More nodes need more iterations, but cap at 500.
    (50 + n * 2).min(500)
}

/// Cooling schedule: linear decay from initial temperature.
fn force_temperature(iteration: usize, max_iterations: usize, k: f32) -> f32 {
    let t0 = k * 10.0; // Initial temperature
    let progress = iteration as f32 / max_iterations as f32;
    t0 * (1.0 - progress)
}

/// Compute force displacements for all nodes.
///
/// Uses direct O(n^2) repulsive forces. For graphs > 100 nodes, uses
/// Barnes-Hut grid approximation.
fn force_compute_displacements(
    positions: &[(f32, f32)],
    node_sizes: &[(f32, f32)],
    adjacency: &[Vec<usize>],
    cluster_membership: &[Option<usize>],
    k: f32,
    n: usize,
) -> Vec<(f32, f32)> {
    let mut displacements = vec![(0.0_f32, 0.0_f32); n];
    let k_sq = k * k;

    if n <= 100 {
        // Direct O(n^2) repulsive forces.
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = positions[i].0 - positions[j].0;
                let dy = positions[i].1 - positions[j].1;
                let dist_sq = (dx * dx + dy * dy).max(1.0);
                // Fruchterman-Reingold repulsive force: k^2 / d
                let force = k_sq / dist_sq.sqrt();
                let fx = dx / dist_sq.sqrt() * force;
                let fy = dy / dist_sq.sqrt() * force;
                displacements[i].0 += fx;
                displacements[i].1 += fy;
                displacements[j].0 -= fx;
                displacements[j].1 -= fy;
            }
        }
    } else {
        // Barnes-Hut grid approximation for large graphs.
        force_barnes_hut_repulsion(positions, k_sq, &mut displacements);
    }

    // Attractive forces along edges (Hooke's law).
    for (i, neighbors) in adjacency.iter().enumerate() {
        for &j in neighbors {
            if j <= i {
                continue; // Process each edge once.
            }
            let dx = positions[i].0 - positions[j].0;
            let dy = positions[i].1 - positions[j].1;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            // Fruchterman-Reingold attractive force: d^2 / k
            let force = dist / k;
            let fx = dx / dist * force;
            let fy = dy / dist * force;
            displacements[i].0 -= fx;
            displacements[i].1 -= fy;
            displacements[j].0 += fx;
            displacements[j].1 += fy;
        }
    }

    // Cluster cohesion: extra attractive force toward cluster centroid.
    force_cluster_cohesion(
        positions,
        node_sizes,
        cluster_membership,
        k,
        &mut displacements,
    );

    displacements
}

/// Barnes-Hut grid-based approximation for repulsive forces.
///
/// Divides the space into a grid and computes repulsive forces from
/// grid cell centroids for distant nodes.
fn force_barnes_hut_repulsion(
    positions: &[(f32, f32)],
    k_sq: f32,
    displacements: &mut [(f32, f32)],
) {
    let n = positions.len();
    // Find bounding box.
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for &(x, y) in positions {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    let range_x = (max_x - min_x).max(1.0);
    let range_y = (max_y - min_y).max(1.0);

    // Grid size: roughly sqrt(n) cells per side.
    let grid_size = (n as f32).sqrt().ceil() as usize;
    let cell_w = range_x / grid_size as f32;
    let cell_h = range_y / grid_size as f32;

    // Assign nodes to grid cells and compute cell centroids.
    let mut cell_sum_x = vec![0.0_f32; grid_size * grid_size];
    let mut cell_sum_y = vec![0.0_f32; grid_size * grid_size];
    let mut cell_count = vec![0_u32; grid_size * grid_size];
    let mut node_cell = vec![0_usize; n];

    for (i, &(x, y)) in positions.iter().enumerate() {
        let cx = ((x - min_x) / cell_w).floor() as usize;
        let cy = ((y - min_y) / cell_h).floor() as usize;
        let cx = cx.min(grid_size - 1);
        let cy = cy.min(grid_size - 1);
        let cell_idx = cy * grid_size + cx;
        node_cell[i] = cell_idx;
        cell_sum_x[cell_idx] += x;
        cell_sum_y[cell_idx] += y;
        cell_count[cell_idx] += 1;
    }

    // Compute centroids.
    let mut centroids = vec![(0.0_f32, 0.0_f32, 0_u32); grid_size * grid_size];
    for idx in 0..(grid_size * grid_size) {
        if cell_count[idx] > 0 {
            centroids[idx] = (
                cell_sum_x[idx] / cell_count[idx] as f32,
                cell_sum_y[idx] / cell_count[idx] as f32,
                cell_count[idx],
            );
        }
    }

    let theta_sq: f32 = 1.5; // Barnes-Hut opening angle threshold squared

    for i in 0..n {
        let (px, py) = positions[i];
        let my_cell = node_cell[i];

        for (cell_idx, &(cx, cy, count)) in centroids.iter().enumerate() {
            if count == 0 {
                continue;
            }

            if cell_idx == my_cell {
                // Same cell: compute direct forces.
                for j in 0..n {
                    if node_cell[j] != my_cell || j == i {
                        continue;
                    }
                    let dx = px - positions[j].0;
                    let dy = py - positions[j].1;
                    let dist_sq = (dx * dx + dy * dy).max(1.0);
                    let force = k_sq / dist_sq.sqrt();
                    let dist = dist_sq.sqrt();
                    displacements[i].0 += dx / dist * force;
                    displacements[i].1 += dy / dist * force;
                }
            } else {
                // Different cell: check if far enough for approximation.
                let dx = px - cx;
                let dy = py - cy;
                let dist_sq = (dx * dx + dy * dy).max(1.0);
                let cell_size_sq = cell_w * cell_w + cell_h * cell_h;

                if cell_size_sq / dist_sq < theta_sq {
                    // Use centroid approximation (multiply force by count).
                    let force = k_sq * count as f32 / dist_sq.sqrt();
                    let dist = dist_sq.sqrt();
                    displacements[i].0 += dx / dist * force;
                    displacements[i].1 += dy / dist * force;
                } else {
                    // Too close: compute direct forces.
                    for j in 0..n {
                        if node_cell[j] != cell_idx || j == i {
                            continue;
                        }
                        let dx2 = px - positions[j].0;
                        let dy2 = py - positions[j].1;
                        let dist_sq2 = (dx2 * dx2 + dy2 * dy2).max(1.0);
                        let force2 = k_sq / dist_sq2.sqrt();
                        let dist2 = dist_sq2.sqrt();
                        displacements[i].0 += dx2 / dist2 * force2;
                        displacements[i].1 += dy2 / dist2 * force2;
                    }
                }
            }
        }
    }
}

/// Apply extra attractive force for nodes in the same cluster.
fn force_cluster_cohesion(
    positions: &[(f32, f32)],
    _node_sizes: &[(f32, f32)],
    cluster_membership: &[Option<usize>],
    k: f32,
    displacements: &mut [(f32, f32)],
) {
    // Compute cluster centroids.
    let mut cluster_sum: BTreeMap<usize, (f32, f32, usize)> = BTreeMap::new();
    for (i, &membership) in cluster_membership.iter().enumerate() {
        if let Some(ci) = membership {
            let entry = cluster_sum.entry(ci).or_insert((0.0, 0.0, 0));
            entry.0 += positions[i].0;
            entry.1 += positions[i].1;
            entry.2 += 1;
        }
    }

    let cohesion_strength = 0.3; // Extra pull toward cluster center

    for (i, &membership) in cluster_membership.iter().enumerate() {
        if let Some(ci) = membership
            && let Some(&(sx, sy, count)) = cluster_sum.get(&ci)
            && count > 1
        {
            let centroid_x = sx / count as f32;
            let centroid_y = sy / count as f32;
            let dx = centroid_x - positions[i].0;
            let dy = centroid_y - positions[i].1;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let force = dist / k * cohesion_strength;
            displacements[i].0 += dx / dist * force;
            displacements[i].1 += dy / dist * force;
        }
    }
}

/// Remove node overlaps via iterative projection.
fn force_remove_overlaps(
    positions: &mut [(f32, f32)],
    node_sizes: &[(f32, f32)],
    spacing: &LayoutSpacing,
) {
    let n = positions.len();
    let gap = spacing.node_spacing * 0.25; // Minimum gap between nodes

    for _pass in 0..20 {
        let mut any_overlap = false;
        for i in 0..n {
            for j in (i + 1)..n {
                let (wi, hi) = node_sizes[i];
                let (wj, hj) = node_sizes[j];
                let half_w = (wi + wj) / 2.0 + gap;
                let half_h = (hi + hj) / 2.0 + gap;

                let dx = positions[j].0 - positions[i].0;
                let dy = positions[j].1 - positions[i].1;
                let overlap_x = half_w - dx.abs();
                let overlap_y = half_h - dy.abs();

                if overlap_x > 0.0 && overlap_y > 0.0 {
                    any_overlap = true;
                    // Push apart along the axis with less overlap.
                    if overlap_x < overlap_y {
                        let push = overlap_x / 2.0;
                        if dx >= 0.0 {
                            positions[i].0 -= push;
                            positions[j].0 += push;
                        } else {
                            positions[i].0 += push;
                            positions[j].0 -= push;
                        }
                    } else {
                        let push = overlap_y / 2.0;
                        if dy >= 0.0 {
                            positions[i].1 -= push;
                            positions[j].1 += push;
                        } else {
                            positions[i].1 += push;
                            positions[j].1 -= push;
                        }
                    }
                }
            }
        }
        if !any_overlap {
            break;
        }
    }
}

/// Normalize positions so all coordinates are non-negative.
fn force_normalize_positions(positions: &mut [(f32, f32)], node_sizes: &[(f32, f32)]) {
    if positions.is_empty() {
        return;
    }
    let margin = 20.0;
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    for (i, &(x, y)) in positions.iter().enumerate() {
        let (w, h) = node_sizes[i];
        min_x = min_x.min(x - w / 2.0);
        min_y = min_y.min(y - h / 2.0);
    }
    let offset_x = margin - min_x;
    let offset_y = margin - min_y;
    for pos in positions.iter_mut() {
        pos.0 += offset_x;
        pos.1 += offset_y;
    }
}

/// Build LayoutNodeBox from force-directed positions (center-based).
fn force_build_node_boxes(
    ir: &MermaidDiagramIr,
    positions: &[(f32, f32)],
    node_sizes: &[(f32, f32)],
) -> Vec<LayoutNodeBox> {
    ir.nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let (cx, cy) = positions[i];
            let (w, h) = node_sizes[i];
            LayoutNodeBox {
                node_index: i,
                node_id: node.id.clone(),
                rank: 0,  // No ranks in force-directed layout.
                order: i, // Order by index.
                bounds: LayoutRect {
                    x: cx - w / 2.0,
                    y: cy - h / 2.0,
                    width: w,
                    height: h,
                },
            }
        })
        .collect()
}

/// Build straight-line edge paths for force-directed layout.
fn force_build_edge_paths(ir: &MermaidDiagramIr, nodes: &[LayoutNodeBox]) -> Vec<LayoutEdgePath> {
    ir.edges
        .iter()
        .enumerate()
        .filter_map(|(ei, edge)| {
            let from_idx = endpoint_node_index(ir, edge.from)?;
            let to_idx = endpoint_node_index(ir, edge.to)?;
            if from_idx >= nodes.len() || to_idx >= nodes.len() {
                return None;
            }
            let from_center = nodes[from_idx].bounds.center();
            let to_center = nodes[to_idx].bounds.center();

            // Clip to node boundaries.
            let from_pt = clip_to_rect_border(from_center, to_center, &nodes[from_idx].bounds);
            let to_pt = clip_to_rect_border(to_center, from_center, &nodes[to_idx].bounds);

            Some(LayoutEdgePath {
                edge_index: ei,
                points: vec![from_pt, to_pt],
                reversed: false,
                is_self_loop: from_idx == to_idx,
                parallel_offset: 0.0,
            })
        })
        .collect()
}

/// Clip a line from `from` toward `to` to the border of `rect`.
fn clip_to_rect_border(from: LayoutPoint, to: LayoutPoint, rect: &LayoutRect) -> LayoutPoint {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let dx = to.x - from.x;
    let dy = to.y - from.y;

    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return from;
    }

    let half_w = rect.width / 2.0;
    let half_h = rect.height / 2.0;

    // Find intersection with rect border along direction (dx, dy) from center.
    let tx = if dx.abs() > f32::EPSILON {
        half_w / dx.abs()
    } else {
        f32::MAX
    };
    let ty = if dy.abs() > f32::EPSILON {
        half_h / dy.abs()
    } else {
        f32::MAX
    };
    let t = tx.min(ty);

    LayoutPoint {
        x: cx + dx * t,
        y: cy + dy * t,
    }
}

#[must_use]
pub fn compute_node_sizes(ir: &MermaidDiagramIr) -> Vec<(f32, f32)> {
    ir.nodes
        .iter()
        .map(|node| {
            let (max_len, lines) = label_length_and_lines(ir, node);
            let label_width = (max_len.max(4) as f32) * 8.0;
            let width = label_width.max(72.0);
            let height = 40.0 + ((lines.saturating_sub(1) as f32) * 16.0);
            (width, height)
        })
        .collect()
}

fn label_length_and_lines(ir: &MermaidDiagramIr, node: &fm_core::IrNode) -> (usize, usize) {
    let text = node
        .label
        .and_then(|label_id| ir.labels.get(label_id.0))
        .map(|value| value.text.as_str())
        .unwrap_or_else(|| node.id.as_str());

    let lines = text.lines().count().max(1);
    let max_len = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);

    (max_len, lines)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CycleRemovalResult {
    reversed_edge_indexes: BTreeSet<usize>,
    highlighted_edge_indexes: BTreeSet<usize>,
    summary: CycleSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct CycleSummary {
    cycle_count: usize,
    cycle_node_count: usize,
    max_cycle_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CycleDetection {
    components: Vec<Vec<usize>>,
    node_to_component: Vec<Option<usize>>,
    cyclic_component_indexes: BTreeSet<usize>,
    summary: CycleSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CycleClusterMap {
    /// For each original node index, the representative node index (self if not collapsed).
    node_representative: Vec<usize>,
    /// The set of representative node indexes that are cycle cluster heads.
    cluster_heads: BTreeSet<usize>,
    /// For each cluster head, the list of member node indexes (including the head).
    cluster_members: BTreeMap<usize, Vec<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OrientedEdge {
    source: usize,
    target: usize,
    edge_index: usize,
}

fn default_cycle_strategy() -> CycleStrategy {
    std::env::var("FM_CYCLE_STRATEGY")
        .ok()
        .as_deref()
        .and_then(CycleStrategy::parse)
        .unwrap_or_default()
}

fn cycle_removal(ir: &MermaidDiagramIr, cycle_strategy: CycleStrategy) -> CycleRemovalResult {
    let node_count = ir.nodes.len();
    if node_count == 0 {
        return CycleRemovalResult {
            reversed_edge_indexes: BTreeSet::new(),
            highlighted_edge_indexes: BTreeSet::new(),
            summary: CycleSummary::default(),
        };
    }

    let edges = resolved_edges(ir);
    if edges.is_empty() {
        return CycleRemovalResult {
            reversed_edge_indexes: BTreeSet::new(),
            highlighted_edge_indexes: BTreeSet::new(),
            summary: CycleSummary::default(),
        };
    }

    let node_priority = stable_node_priorities(ir);
    let cycle_detection = detect_cycle_components(node_count, &edges, &node_priority);
    let dfs_back_edges = cycle_removal_dfs_back(node_count, &edges, &node_priority);

    let reversed_edge_indexes = match cycle_strategy {
        CycleStrategy::Greedy => cycle_removal_greedy(node_count, &edges, &node_priority),
        CycleStrategy::DfsBack => dfs_back_edges.clone(),
        CycleStrategy::MfasApprox => {
            cycle_removal_mfas_approx(node_count, &edges, &node_priority, &cycle_detection)
        }
        CycleStrategy::CycleAware => BTreeSet::new(),
    };

    let highlighted_edge_indexes = if matches!(cycle_strategy, CycleStrategy::CycleAware) {
        dfs_back_edges
    } else {
        reversed_edge_indexes.clone()
    };

    CycleRemovalResult {
        reversed_edge_indexes,
        highlighted_edge_indexes,
        summary: cycle_detection.summary,
    }
}

fn detect_cycle_components(
    node_count: usize,
    edges: &[OrientedEdge],
    node_priority: &[usize],
) -> CycleDetection {
    struct TarjanState<'a> {
        index: usize,
        indices: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        components: Vec<Vec<usize>>,
        outgoing_edge_slots: &'a [Vec<usize>],
        edges: &'a [OrientedEdge],
        node_priority: &'a [usize],
    }

    impl TarjanState<'_> {
        fn strong_connect(&mut self, node: usize) {
            self.indices[node] = Some(self.index);
            self.lowlink[node] = self.index;
            self.index = self.index.saturating_add(1);
            self.stack.push(node);
            self.on_stack[node] = true;

            for edge_slot in self.outgoing_edge_slots[node].iter().copied() {
                let next = self.edges[edge_slot].target;
                if self.indices[next].is_none() {
                    self.strong_connect(next);
                    self.lowlink[node] = self.lowlink[node].min(self.lowlink[next]);
                } else if self.on_stack[next] {
                    self.lowlink[node] =
                        self.lowlink[node].min(self.indices[next].unwrap_or(self.lowlink[node]));
                }
            }

            if self.lowlink[node] == self.indices[node].unwrap_or(self.lowlink[node]) {
                let mut component = Vec::new();
                while let Some(top) = self.stack.pop() {
                    self.on_stack[top] = false;
                    component.push(top);
                    if top == node {
                        break;
                    }
                }
                component
                    .sort_by(|left, right| compare_priority(*left, *right, self.node_priority));
                self.components.push(component);
            }
        }
    }

    let outgoing_edge_slots = sorted_outgoing_edge_slots(node_count, edges, node_priority);
    let mut tarjan = TarjanState {
        index: 0,
        indices: vec![None; node_count],
        lowlink: vec![0_usize; node_count],
        stack: Vec::new(),
        on_stack: vec![false; node_count],
        components: Vec::new(),
        outgoing_edge_slots: &outgoing_edge_slots,
        edges,
        node_priority,
    };

    let mut node_visit_order: Vec<usize> = (0..node_count).collect();
    node_visit_order.sort_by(|left, right| compare_priority(*left, *right, node_priority));
    for node in node_visit_order {
        if tarjan.indices[node].is_none() {
            tarjan.strong_connect(node);
        }
    }

    let mut node_to_component = vec![None; node_count];
    for (component_index, component_nodes) in tarjan.components.iter().enumerate() {
        for node in component_nodes {
            node_to_component[*node] = Some(component_index);
        }
    }

    let mut cyclic_component_indexes = BTreeSet::new();
    let mut cycle_node_count = 0_usize;
    let mut max_cycle_size = 0_usize;
    for (component_index, component_nodes) in tarjan.components.iter().enumerate() {
        let is_cyclic = if component_nodes.len() > 1 {
            true
        } else {
            let node = component_nodes[0];
            edges
                .iter()
                .any(|edge| edge.source == node && edge.target == node)
        };

        if is_cyclic {
            cyclic_component_indexes.insert(component_index);
            cycle_node_count = cycle_node_count.saturating_add(component_nodes.len());
            max_cycle_size = max_cycle_size.max(component_nodes.len());
        }
    }

    CycleDetection {
        components: tarjan.components,
        node_to_component,
        cyclic_component_indexes: cyclic_component_indexes.clone(),
        summary: CycleSummary {
            cycle_count: cyclic_component_indexes.len(),
            cycle_node_count,
            max_cycle_size,
        },
    }
}

fn cycle_removal_dfs_back(
    node_count: usize,
    edges: &[OrientedEdge],
    node_priority: &[usize],
) -> BTreeSet<usize> {
    let outgoing_edge_slots = sorted_outgoing_edge_slots(node_count, edges, node_priority);
    let mut state = vec![0_u8; node_count];
    let mut reversed_edge_indexes = BTreeSet::new();

    fn visit(
        node: usize,
        state: &mut [u8],
        outgoing_edge_slots: &[Vec<usize>],
        edges: &[OrientedEdge],
        reversed_edge_indexes: &mut BTreeSet<usize>,
    ) {
        state[node] = 1;
        for edge_slot in outgoing_edge_slots[node].iter().copied() {
            let edge = edges[edge_slot];
            match state[edge.target] {
                0 => visit(
                    edge.target,
                    state,
                    outgoing_edge_slots,
                    edges,
                    reversed_edge_indexes,
                ),
                1 => {
                    reversed_edge_indexes.insert(edge.edge_index);
                }
                _ => {}
            }
        }
        state[node] = 2;
    }

    let mut node_visit_order: Vec<usize> = (0..node_count).collect();
    node_visit_order.sort_by(|left, right| compare_priority(*left, *right, node_priority));
    for node in node_visit_order {
        if state[node] == 0 {
            visit(
                node,
                &mut state,
                &outgoing_edge_slots,
                edges,
                &mut reversed_edge_indexes,
            );
        }
    }

    reversed_edge_indexes
}

fn cycle_removal_mfas_approx(
    node_count: usize,
    edges: &[OrientedEdge],
    node_priority: &[usize],
    cycle_detection: &CycleDetection,
) -> BTreeSet<usize> {
    if cycle_detection.summary.cycle_count == 0 {
        return BTreeSet::new();
    }

    let mut reversed_edge_indexes = BTreeSet::new();

    for component_index in &cycle_detection.cyclic_component_indexes {
        let component_nodes = cycle_detection
            .components
            .get(*component_index)
            .cloned()
            .unwrap_or_default();
        if component_nodes.is_empty() {
            continue;
        }

        let mut in_degree = vec![0_usize; node_count];
        let mut out_degree = vec![0_usize; node_count];

        for edge in edges {
            if cycle_detection.node_to_component[edge.source] == Some(*component_index)
                && cycle_detection.node_to_component[edge.target] == Some(*component_index)
            {
                out_degree[edge.source] = out_degree[edge.source].saturating_add(1);
                in_degree[edge.target] = in_degree[edge.target].saturating_add(1);
            }
        }

        let mut component_order = component_nodes;
        component_order.sort_by(|left, right| {
            let left_score = out_degree[*left] as isize - in_degree[*left] as isize;
            let right_score = out_degree[*right] as isize - in_degree[*right] as isize;
            right_score
                .cmp(&left_score)
                .then_with(|| compare_priority(*left, *right, node_priority))
        });

        let mut position = BTreeMap::<usize, usize>::new();
        for (index, node) in component_order.into_iter().enumerate() {
            position.insert(node, index);
        }

        for edge in edges {
            if cycle_detection.node_to_component[edge.source] == Some(*component_index)
                && cycle_detection.node_to_component[edge.target] == Some(*component_index)
                && position.get(&edge.source).copied().unwrap_or(0)
                    > position.get(&edge.target).copied().unwrap_or(0)
            {
                reversed_edge_indexes.insert(edge.edge_index);
            }
        }
    }

    if reversed_edge_indexes.is_empty() {
        return cycle_removal_dfs_back(node_count, edges, node_priority);
    }

    reversed_edge_indexes
}

fn sorted_outgoing_edge_slots(
    node_count: usize,
    edges: &[OrientedEdge],
    node_priority: &[usize],
) -> Vec<Vec<usize>> {
    let mut outgoing_edge_slots = vec![Vec::new(); node_count];
    for (edge_slot, edge) in edges.iter().enumerate() {
        outgoing_edge_slots[edge.source].push(edge_slot);
    }

    for slots in &mut outgoing_edge_slots {
        slots.sort_by(|left, right| {
            let left_edge = edges[*left];
            let right_edge = edges[*right];
            compare_priority(left_edge.target, right_edge.target, node_priority)
                .then_with(|| left_edge.edge_index.cmp(&right_edge.edge_index))
        });
    }

    outgoing_edge_slots
}

fn cycle_removal_greedy(
    node_count: usize,
    edges: &[OrientedEdge],
    node_priority: &[usize],
) -> BTreeSet<usize> {
    let mut active_nodes: BTreeSet<usize> = (0..node_count).collect();
    let mut in_degree = vec![0_usize; node_count];
    let mut out_degree = vec![0_usize; node_count];
    let mut incoming = vec![Vec::new(); node_count];
    let mut outgoing = vec![Vec::new(); node_count];

    for (edge_slot, edge) in edges.iter().enumerate() {
        in_degree[edge.target] = in_degree[edge.target].saturating_add(1);
        out_degree[edge.source] = out_degree[edge.source].saturating_add(1);
        incoming[edge.target].push(edge_slot);
        outgoing[edge.source].push(edge_slot);
    }

    let mut left_order = Vec::with_capacity(node_count);
    let mut right_order = Vec::with_capacity(node_count);

    while !active_nodes.is_empty() {
        let mut sinks: Vec<usize> = active_nodes
            .iter()
            .copied()
            .filter(|node| out_degree[*node] == 0)
            .collect();
        if !sinks.is_empty() {
            sinks.sort_by(|left, right| compare_priority(*left, *right, node_priority));
            for node in sinks {
                remove_node(
                    node,
                    &mut active_nodes,
                    &incoming,
                    &outgoing,
                    edges,
                    &mut in_degree,
                    &mut out_degree,
                );
                right_order.push(node);
            }
            continue;
        }

        let mut sources: Vec<usize> = active_nodes
            .iter()
            .copied()
            .filter(|node| in_degree[*node] == 0)
            .collect();
        if !sources.is_empty() {
            sources.sort_by(|left, right| compare_priority(*left, *right, node_priority));
            for node in sources {
                remove_node(
                    node,
                    &mut active_nodes,
                    &incoming,
                    &outgoing,
                    edges,
                    &mut in_degree,
                    &mut out_degree,
                );
                left_order.push(node);
            }
            continue;
        }

        let Some(candidate) = active_nodes.iter().copied().max_by(|left, right| {
            let left_score = out_degree[*left] as isize - in_degree[*left] as isize;
            let right_score = out_degree[*right] as isize - in_degree[*right] as isize;
            left_score
                .cmp(&right_score)
                .then_with(|| compare_priority(*right, *left, node_priority))
        }) else {
            break;
        };

        remove_node(
            candidate,
            &mut active_nodes,
            &incoming,
            &outgoing,
            edges,
            &mut in_degree,
            &mut out_degree,
        );
        left_order.push(candidate);
    }

    left_order.extend(right_order.into_iter().rev());
    let mut position = vec![0_usize; node_count];
    for (order, node_index) in left_order.into_iter().enumerate() {
        position[node_index] = order;
    }

    edges
        .iter()
        .filter_map(|edge| {
            (position[edge.source] > position[edge.target]).then_some(edge.edge_index)
        })
        .collect()
}

fn rank_assignment(ir: &MermaidDiagramIr, cycles: &CycleRemovalResult) -> BTreeMap<usize, usize> {
    let node_count = ir.nodes.len();
    let node_priority = stable_node_priorities(ir);
    let edges = oriented_edges(ir, &cycles.reversed_edge_indexes);

    let mut ranks = vec![0_usize; node_count];
    let mut in_degree = vec![0_usize; node_count];
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); node_count];

    for edge in &edges {
        if edge.source == edge.target {
            continue;
        }
        in_degree[edge.target] = in_degree[edge.target].saturating_add(1);
        outgoing[edge.source].push(edge.target);
    }

    for targets in &mut outgoing {
        targets.sort_by(|left, right| compare_priority(*left, *right, &node_priority));
    }

    let mut heap: BinaryHeap<Reverse<(usize, usize)>> = BinaryHeap::new();
    for node_index in 0..node_count {
        if in_degree[node_index] == 0 {
            heap.push(Reverse((node_priority[node_index], node_index)));
        }
    }

    let mut visited = 0_usize;
    while let Some(Reverse((_priority, node_index))) = heap.pop() {
        visited = visited.saturating_add(1);
        let source_rank = ranks[node_index];

        for target in outgoing[node_index].iter().copied() {
            let candidate_rank = source_rank.saturating_add(1);
            if candidate_rank > ranks[target] {
                ranks[target] = candidate_rank;
            }
            in_degree[target] = in_degree[target].saturating_sub(1);
            if in_degree[target] == 0 {
                heap.push(Reverse((node_priority[target], target)));
            }
        }
    }

    if visited < node_count {
        // Residual cyclic components fallback to bounded longest-path relaxation.
        let guard = edges.len().saturating_mul(2).saturating_add(1);
        for _ in 0..guard {
            let mut changed = false;
            for edge in &edges {
                if edge.source == edge.target {
                    continue;
                }
                let candidate_rank = ranks[edge.source].saturating_add(1);
                if candidate_rank > ranks[edge.target] {
                    ranks[edge.target] = candidate_rank;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    // Compact disconnected components along the rank axis so each component
    // gets an independent band instead of sharing rank-0/rank-1 globally.
    // This avoids pathological ultra-wide layouts for many disconnected chains.
    let mut components = weakly_connected_components(node_count, &edges);
    components.sort_by_key(|component| {
        component
            .iter()
            .map(|node_index| node_priority[*node_index])
            .min()
            .unwrap_or(usize::MAX)
    });

    if components.len() > 1 && !edges.is_empty() {
        let mut compacted_ranks = ranks.clone();
        let mut isolated_singletons = Vec::new();
        let mut incident_edge_count = vec![0_usize; node_count];
        for edge in &edges {
            if edge.source < node_count {
                incident_edge_count[edge.source] =
                    incident_edge_count[edge.source].saturating_add(1);
            }
            if edge.target < node_count {
                incident_edge_count[edge.target] =
                    incident_edge_count[edge.target].saturating_add(1);
            }
        }
        let mut rank_cursor = 0_usize;

        for component in components {
            if component.is_empty() {
                continue;
            }
            if component.len() == 1 && incident_edge_count[component[0]] == 0 {
                isolated_singletons.push(component[0]);
                continue;
            }

            let mut min_rank = usize::MAX;
            let mut max_rank = 0_usize;
            for &node_index in &component {
                let rank = ranks[node_index];
                min_rank = min_rank.min(rank);
                max_rank = max_rank.max(rank);
            }

            if min_rank == usize::MAX {
                continue;
            }

            let span = max_rank.saturating_sub(min_rank).saturating_add(1);
            for &node_index in &component {
                compacted_ranks[node_index] = ranks[node_index]
                    .saturating_sub(min_rank)
                    .saturating_add(rank_cursor);
            }

            rank_cursor = rank_cursor.saturating_add(span).saturating_add(1);
        }

        if !isolated_singletons.is_empty() {
            for node_index in isolated_singletons {
                compacted_ranks[node_index] = rank_cursor;
            }
        }

        ranks = compacted_ranks;
    }

    (0..node_count).map(|index| (index, ranks[index])).collect()
}

fn weakly_connected_components(node_count: usize, edges: &[OrientedEdge]) -> Vec<Vec<usize>> {
    if node_count == 0 {
        return Vec::new();
    }

    let mut adjacency: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); node_count];
    for edge in edges {
        if edge.source >= node_count || edge.target >= node_count {
            continue;
        }
        adjacency[edge.source].insert(edge.target);
        adjacency[edge.target].insert(edge.source);
    }

    let mut visited = vec![false; node_count];
    let mut components = Vec::new();

    for start in 0..node_count {
        if visited[start] {
            continue;
        }

        let mut stack = vec![start];
        visited[start] = true;
        let mut component = Vec::new();

        while let Some(node_index) = stack.pop() {
            component.push(node_index);
            for &neighbor in adjacency[node_index].iter().rev() {
                if visited[neighbor] {
                    continue;
                }
                visited[neighbor] = true;
                stack.push(neighbor);
            }
        }

        component.sort_unstable();
        components.push(component);
    }

    components
}

fn resolved_edges(ir: &MermaidDiagramIr) -> Vec<OrientedEdge> {
    ir.edges
        .iter()
        .enumerate()
        .filter_map(|(edge_index, edge)| {
            let source = endpoint_node_index(ir, edge.from)?;
            let target = endpoint_node_index(ir, edge.to)?;
            Some(OrientedEdge {
                source,
                target,
                edge_index,
            })
        })
        .collect()
}

fn oriented_edges(
    ir: &MermaidDiagramIr,
    reversed_edge_indexes: &BTreeSet<usize>,
) -> Vec<OrientedEdge> {
    resolved_edges(ir)
        .into_iter()
        .map(|mut edge| {
            if reversed_edge_indexes.contains(&edge.edge_index) {
                std::mem::swap(&mut edge.source, &mut edge.target);
            }
            edge
        })
        .collect()
}

fn stable_node_priorities(ir: &MermaidDiagramIr) -> Vec<usize> {
    let mut node_indexes: Vec<usize> = (0..ir.nodes.len()).collect();
    node_indexes.sort_by(|left, right| compare_node_indices(ir, *left, *right));

    let mut priorities = vec![0_usize; ir.nodes.len()];
    for (priority, node_index) in node_indexes.into_iter().enumerate() {
        priorities[node_index] = priority;
    }
    priorities
}

fn compare_node_indices(ir: &MermaidDiagramIr, left: usize, right: usize) -> std::cmp::Ordering {
    ir.nodes[left]
        .id
        .cmp(&ir.nodes[right].id)
        .then_with(|| left.cmp(&right))
}

fn compare_priority(left: usize, right: usize, node_priority: &[usize]) -> std::cmp::Ordering {
    node_priority[left]
        .cmp(&node_priority[right])
        .then_with(|| left.cmp(&right))
}

fn remove_node(
    node: usize,
    active_nodes: &mut BTreeSet<usize>,
    incoming: &[Vec<usize>],
    outgoing: &[Vec<usize>],
    edges: &[OrientedEdge],
    in_degree: &mut [usize],
    out_degree: &mut [usize],
) {
    if !active_nodes.remove(&node) {
        return;
    }

    for edge_slot in outgoing[node].iter().copied() {
        let target = edges[edge_slot].target;
        if active_nodes.contains(&target) {
            in_degree[target] = in_degree[target].saturating_sub(1);
        }
    }

    for edge_slot in incoming[node].iter().copied() {
        let source = edges[edge_slot].source;
        if active_nodes.contains(&source) {
            out_degree[source] = out_degree[source].saturating_sub(1);
        }
    }
}

fn crossing_minimization(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
) -> (usize, BTreeMap<usize, Vec<usize>>) {
    let mut ordering_by_rank = nodes_by_rank(ir.nodes.len(), ranks);
    if ordering_by_rank.len() <= 1 {
        return (0, ordering_by_rank);
    }

    // Deterministic barycenter sweeps: top-down then bottom-up.
    let rank_keys: Vec<usize> = ordering_by_rank.keys().copied().collect();
    for _ in 0..4 {
        for index in 1..rank_keys.len() {
            let rank = rank_keys[index];
            let upper_rank = rank_keys[index - 1];
            reorder_rank_by_barycenter(ir, ranks, &mut ordering_by_rank, rank, upper_rank, true);
        }

        for index in (0..rank_keys.len().saturating_sub(1)).rev() {
            let rank = rank_keys[index];
            let lower_rank = rank_keys[index + 1];
            reorder_rank_by_barycenter(ir, ranks, &mut ordering_by_rank, rank, lower_rank, false);
        }
    }

    let crossing_count = total_crossings(ir, ranks, &ordering_by_rank);
    (crossing_count, ordering_by_rank)
}

/// Apply transpose and sifting refinement heuristics to reduce crossings
/// beyond what barycenter achieves alone.
fn crossing_refinement(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    mut ordering_by_rank: BTreeMap<usize, Vec<usize>>,
    mut best_crossings: usize,
) -> (usize, BTreeMap<usize, Vec<usize>>) {
    if best_crossings == 0 {
        return (0, ordering_by_rank);
    }

    // Phase 1: Transpose — swap adjacent nodes in each rank if it reduces crossings.
    let mut improved = true;
    for _pass in 0..10 {
        if !improved {
            break;
        }
        improved = false;
        let rank_keys: Vec<usize> = ordering_by_rank.keys().copied().collect();
        for &rank in &rank_keys {
            let order = match ordering_by_rank.get(&rank) {
                Some(o) if o.len() >= 2 => o.clone(),
                _ => continue,
            };
            for i in 0..order.len() - 1 {
                // Try swapping positions i and i+1.
                let mut trial = ordering_by_rank.clone();
                if let Some(rank_order) = trial.get_mut(&rank) {
                    rank_order.swap(i, i + 1);
                }
                let trial_crossings = total_crossings(ir, ranks, &trial);
                if trial_crossings < best_crossings {
                    ordering_by_rank = trial;
                    best_crossings = trial_crossings;
                    improved = true;
                    if best_crossings == 0 {
                        return (0, ordering_by_rank);
                    }
                }
            }
        }
    }

    // Phase 2: Sifting — for each node in each rank, try every position in that rank.
    let rank_keys: Vec<usize> = ordering_by_rank.keys().copied().collect();
    for &rank in &rank_keys {
        let order = match ordering_by_rank.get(&rank) {
            Some(o) if o.len() >= 3 => o.clone(),
            _ => continue,
        };
        let n = order.len();
        for node_orig_pos in 0..n {
            let node = order[node_orig_pos];
            let mut best_pos = node_orig_pos;
            for target_pos in 0..n {
                if target_pos == best_pos {
                    continue;
                }
                // Build trial ordering with node moved to target_pos.
                let mut trial_order: Vec<usize> =
                    order.iter().copied().filter(|&ni| ni != node).collect();
                trial_order.insert(target_pos.min(trial_order.len()), node);

                let mut trial = ordering_by_rank.clone();
                trial.insert(rank, trial_order);
                let trial_crossings = total_crossings(ir, ranks, &trial);
                if trial_crossings < best_crossings {
                    best_crossings = trial_crossings;
                    best_pos = target_pos;
                    ordering_by_rank = trial;
                    if best_crossings == 0 {
                        return (0, ordering_by_rank);
                    }
                }
            }
            // If best_pos changed, update the reference order for subsequent nodes.
            let _ = best_pos; // Already applied via ordering_by_rank = trial above.
        }
    }

    (best_crossings, ordering_by_rank)
}

fn coordinate_assignment(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    ranks: &BTreeMap<usize, usize>,
    ordering_by_rank: &BTreeMap<usize, Vec<usize>>,
    spacing: LayoutSpacing,
) -> Vec<LayoutNodeBox> {
    let fallback_nodes_by_rank = nodes_by_rank(ir.nodes.len(), ranks);
    let horizontal_ranks = matches!(ir.direction, GraphDirection::LR | GraphDirection::RL);
    let reverse_ranks = matches!(ir.direction, GraphDirection::RL | GraphDirection::BT);
    let ordered_ranks: Vec<usize> = fallback_nodes_by_rank.keys().copied().collect();

    let rank_to_index: BTreeMap<usize, usize> = ordered_ranks
        .iter()
        .enumerate()
        .map(|(index, rank)| (*rank, index))
        .collect();

    let mut rank_span = vec![0.0_f32; ordered_ranks.len()];
    for (rank_index, rank) in ordered_ranks.iter().copied().enumerate() {
        let node_indexes = ordering_by_rank
            .get(&rank)
            .cloned()
            .or_else(|| fallback_nodes_by_rank.get(&rank).cloned())
            .unwrap_or_default();

        let mut span = 0.0_f32;
        for node_index in node_indexes {
            let (width, height) = node_sizes.get(node_index).copied().unwrap_or((72.0, 40.0));
            let primary_extent = if horizontal_ranks { width } else { height };
            span = span.max(primary_extent);
        }
        rank_span[rank_index] = span.max(1.0);
    }

    let mut primary_offsets = vec![0.0_f32; ordered_ranks.len()];
    let mut primary_cursor = 0.0_f32;
    let iter_order: Vec<usize> = if reverse_ranks {
        (0..ordered_ranks.len()).rev().collect()
    } else {
        (0..ordered_ranks.len()).collect()
    };
    for rank_index in iter_order {
        primary_offsets[rank_index] = primary_cursor;
        primary_cursor += rank_span[rank_index] + spacing.rank_spacing;
    }

    let mut output = Vec::with_capacity(ir.nodes.len());
    for (rank, fallback_node_indexes) in fallback_nodes_by_rank {
        let Some(rank_index) = rank_to_index.get(&rank).copied() else {
            continue;
        };

        let node_indexes = ordering_by_rank
            .get(&rank)
            .cloned()
            .unwrap_or(fallback_node_indexes);

        let primary = primary_offsets.get(rank_index).copied().unwrap_or(0.0);
        let mut secondary_cursor = 0.0_f32;
        for (order, node_index) in node_indexes.into_iter().enumerate() {
            let (width, height) = node_sizes.get(node_index).copied().unwrap_or((72.0, 40.0));
            let (x, y) = if horizontal_ranks {
                (primary, secondary_cursor)
            } else {
                (secondary_cursor, primary)
            };
            let node_id = ir
                .nodes
                .get(node_index)
                .map(|node| node.id.clone())
                .unwrap_or_default();

            output.push(LayoutNodeBox {
                node_index,
                node_id,
                rank,
                order,
                bounds: LayoutRect {
                    x,
                    y,
                    width,
                    height,
                },
            });

            let secondary_extent = if horizontal_ranks { height } else { width };
            secondary_cursor += secondary_extent + spacing.node_spacing;
        }
    }

    output.sort_by_key(|node| node.node_index);
    output
}

fn nodes_by_rank(node_count: usize, ranks: &BTreeMap<usize, usize>) -> BTreeMap<usize, Vec<usize>> {
    let mut nodes_by_rank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for node_index in 0..node_count {
        let rank = ranks.get(&node_index).copied().unwrap_or(0);
        nodes_by_rank.entry(rank).or_default().push(node_index);
    }
    nodes_by_rank
}

fn reorder_rank_by_barycenter(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    ordering_by_rank: &mut BTreeMap<usize, Vec<usize>>,
    rank: usize,
    adjacent_rank: usize,
    use_incoming: bool,
) {
    let Some(current_order) = ordering_by_rank.get(&rank).cloned() else {
        return;
    };
    let Some(adjacent_order) = ordering_by_rank.get(&adjacent_rank) else {
        return;
    };

    let adjacent_position: BTreeMap<usize, usize> = adjacent_order
        .iter()
        .enumerate()
        .map(|(position, node)| (*node, position))
        .collect();

    let mut scored_nodes: Vec<(usize, Option<f32>, usize)> = current_order
        .iter()
        .enumerate()
        .map(|(stable_idx, node_index)| {
            let mut total_position = 0_usize;
            let mut neighbor_count = 0_usize;

            for edge in &ir.edges {
                let Some(source) = endpoint_node_index(ir, edge.from) else {
                    continue;
                };
                let Some(target) = endpoint_node_index(ir, edge.to) else {
                    continue;
                };

                let neighbor = if use_incoming {
                    if target == *node_index
                        && ranks.get(&source).copied().unwrap_or(0) == adjacent_rank
                    {
                        Some(source)
                    } else {
                        None
                    }
                } else if source == *node_index
                    && ranks.get(&target).copied().unwrap_or(0) == adjacent_rank
                {
                    Some(target)
                } else {
                    None
                };

                if let Some(adjacent_node) = neighbor
                    && let Some(position) = adjacent_position.get(&adjacent_node)
                {
                    total_position = total_position.saturating_add(*position);
                    neighbor_count = neighbor_count.saturating_add(1);
                }
            }

            let barycenter = if neighbor_count == 0 {
                None
            } else {
                Some(total_position as f32 / neighbor_count as f32)
            };
            (*node_index, barycenter, stable_idx)
        })
        .collect();

    scored_nodes.sort_by(|left, right| match (left.1, right.1) {
        (Some(lhs), Some(rhs)) => lhs.total_cmp(&rhs).then_with(|| left.0.cmp(&right.0)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left.2.cmp(&right.2).then_with(|| left.0.cmp(&right.0)),
    });

    ordering_by_rank.insert(
        rank,
        scored_nodes
            .into_iter()
            .map(|(node_index, _, _)| node_index)
            .collect(),
    );
}

fn total_crossings(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    ordering_by_rank: &BTreeMap<usize, Vec<usize>>,
) -> usize {
    let mut positions_by_rank: BTreeMap<usize, BTreeMap<usize, usize>> = BTreeMap::new();
    for (rank, ordered_nodes) in ordering_by_rank {
        positions_by_rank.insert(
            *rank,
            ordered_nodes
                .iter()
                .enumerate()
                .map(|(position, node)| (*node, position))
                .collect(),
        );
    }

    let mut edges_by_layer_pair: BTreeMap<(usize, usize), Vec<(usize, usize)>> = BTreeMap::new();
    for edge in &ir.edges {
        let Some(mut source) = endpoint_node_index(ir, edge.from) else {
            continue;
        };
        let Some(mut target) = endpoint_node_index(ir, edge.to) else {
            continue;
        };
        let Some(mut source_rank) = ranks.get(&source).copied() else {
            continue;
        };
        let Some(mut target_rank) = ranks.get(&target).copied() else {
            continue;
        };

        if source_rank == target_rank {
            continue;
        }
        if source_rank > target_rank {
            std::mem::swap(&mut source, &mut target);
            std::mem::swap(&mut source_rank, &mut target_rank);
        }
        if target_rank != source_rank.saturating_add(1) {
            continue;
        }

        let Some(source_position) = positions_by_rank
            .get(&source_rank)
            .and_then(|positions| positions.get(&source))
            .copied()
        else {
            continue;
        };
        let Some(target_position) = positions_by_rank
            .get(&target_rank)
            .and_then(|positions| positions.get(&target))
            .copied()
        else {
            continue;
        };

        edges_by_layer_pair
            .entry((source_rank, target_rank))
            .or_default()
            .push((source_position, target_position));
    }

    let mut total_crossings = 0_usize;
    for (_layer_pair, mut edge_positions) in edges_by_layer_pair {
        edge_positions
            .sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        let mut target_positions: Vec<usize> = edge_positions
            .into_iter()
            .map(|(_source_position, target_position)| target_position)
            .collect();
        total_crossings = total_crossings.saturating_add(count_inversions(&mut target_positions));
    }

    total_crossings
}

fn count_inversions(values: &mut [usize]) -> usize {
    if values.len() <= 1 {
        return 0;
    }

    let mid = values.len() / 2;
    let mut inversions = 0_usize;
    inversions = inversions.saturating_add(count_inversions(&mut values[..mid]));
    inversions = inversions.saturating_add(count_inversions(&mut values[mid..]));

    let mut merged = Vec::with_capacity(values.len());
    let (left, right) = values.split_at(mid);
    let mut left_idx = 0_usize;
    let mut right_idx = 0_usize;

    while left_idx < left.len() && right_idx < right.len() {
        if left[left_idx] <= right[right_idx] {
            merged.push(left[left_idx]);
            left_idx = left_idx.saturating_add(1);
        } else {
            merged.push(right[right_idx]);
            inversions = inversions.saturating_add(left.len().saturating_sub(left_idx));
            right_idx = right_idx.saturating_add(1);
        }
    }

    merged.extend_from_slice(&left[left_idx..]);
    merged.extend_from_slice(&right[right_idx..]);
    values.copy_from_slice(&merged);
    inversions
}

fn build_edge_paths(
    ir: &MermaidDiagramIr,
    nodes: &[LayoutNodeBox],
    highlighted_edge_indexes: &BTreeSet<usize>,
) -> Vec<LayoutEdgePath> {
    let horizontal_ranks = matches!(ir.direction, GraphDirection::LR | GraphDirection::RL);

    // Track parallel edges: count edges between same (source, target) pair.
    let mut edge_pair_count: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut edge_pair_index: Vec<usize> = Vec::with_capacity(ir.edges.len());
    for edge in &ir.edges {
        let source = endpoint_node_index(ir, edge.from).unwrap_or(usize::MAX);
        let target = endpoint_node_index(ir, edge.to).unwrap_or(usize::MAX);
        let key = (source.min(target), source.max(target));
        let count = edge_pair_count.entry(key).or_insert(0);
        edge_pair_index.push(*count);
        *count += 1;
    }

    ir.edges
        .iter()
        .enumerate()
        .filter_map(|(edge_index, edge)| {
            let source = endpoint_node_index(ir, edge.from)?;
            let target = endpoint_node_index(ir, edge.to)?;
            let source_box = nodes.get(source)?;
            let target_box = nodes.get(target)?;

            let is_self_loop = source == target;
            let key = (source.min(target), source.max(target));
            let pair_total = edge_pair_count.get(&key).copied().unwrap_or(1);
            let pair_idx = edge_pair_index.get(edge_index).copied().unwrap_or(0);
            let parallel_offset = if pair_total > 1 {
                let offset_step = 12.0_f32;
                (pair_idx as f32 - (pair_total - 1) as f32 / 2.0) * offset_step
            } else {
                0.0
            };

            let points = if is_self_loop {
                route_self_loop(source_box, horizontal_ranks)
            } else {
                let (source_anchor, target_anchor) =
                    edge_anchors(source_box, target_box, horizontal_ranks);
                let mut pts = route_edge_points(source_anchor, target_anchor, horizontal_ranks);
                if parallel_offset.abs() > 0.01 {
                    apply_parallel_offset(&mut pts, parallel_offset, horizontal_ranks);
                }
                pts
            };

            Some(LayoutEdgePath {
                edge_index,
                points,
                reversed: highlighted_edge_indexes.contains(&edge_index),
                is_self_loop,
                parallel_offset,
            })
        })
        .collect()
}

/// Route a self-loop edge: goes out one side and returns on another.
fn route_self_loop(node_box: &LayoutNodeBox, horizontal_ranks: bool) -> Vec<LayoutPoint> {
    let b = &node_box.bounds;
    let loop_size = 24.0_f32;

    if horizontal_ranks {
        // Loop goes out the right side and returns from the top.
        let start = LayoutPoint {
            x: b.x + b.width,
            y: b.y + b.height * 0.4,
        };
        let corner1 = LayoutPoint {
            x: b.x + b.width + loop_size,
            y: b.y + b.height * 0.4,
        };
        let corner2 = LayoutPoint {
            x: b.x + b.width + loop_size,
            y: b.y - loop_size,
        };
        let corner3 = LayoutPoint {
            x: b.x + b.width * 0.6,
            y: b.y - loop_size,
        };
        let end = LayoutPoint {
            x: b.x + b.width * 0.6,
            y: b.y,
        };
        vec![start, corner1, corner2, corner3, end]
    } else {
        // Loop goes out the bottom and returns from the right.
        let start = LayoutPoint {
            x: b.x + b.width * 0.6,
            y: b.y + b.height,
        };
        let corner1 = LayoutPoint {
            x: b.x + b.width * 0.6,
            y: b.y + b.height + loop_size,
        };
        let corner2 = LayoutPoint {
            x: b.x + b.width + loop_size,
            y: b.y + b.height + loop_size,
        };
        let corner3 = LayoutPoint {
            x: b.x + b.width + loop_size,
            y: b.y + b.height * 0.4,
        };
        let end = LayoutPoint {
            x: b.x + b.width,
            y: b.y + b.height * 0.4,
        };
        vec![start, corner1, corner2, corner3, end]
    }
}

/// Apply parallel offset to an edge path to distinguish parallel edges.
fn apply_parallel_offset(points: &mut [LayoutPoint], offset: f32, horizontal_ranks: bool) {
    if points.len() < 2 {
        return;
    }
    // Offset perpendicular to the main routing direction.
    for pt in points.iter_mut() {
        if horizontal_ranks {
            pt.y += offset;
        } else {
            pt.x += offset;
        }
    }
}

fn edge_anchors(
    source_box: &LayoutNodeBox,
    target_box: &LayoutNodeBox,
    horizontal_ranks: bool,
) -> (LayoutPoint, LayoutPoint) {
    let source_center = source_box.bounds.center();
    let target_center = target_box.bounds.center();

    if horizontal_ranks {
        let (source_x, target_x) = if target_center.x >= source_center.x {
            (
                source_box.bounds.x + source_box.bounds.width,
                target_box.bounds.x,
            )
        } else {
            (
                source_box.bounds.x,
                target_box.bounds.x + target_box.bounds.width,
            )
        };
        (
            LayoutPoint {
                x: source_x,
                y: source_center.y,
            },
            LayoutPoint {
                x: target_x,
                y: target_center.y,
            },
        )
    } else {
        let (source_y, target_y) = if target_center.y >= source_center.y {
            (
                source_box.bounds.y + source_box.bounds.height,
                target_box.bounds.y,
            )
        } else {
            (
                source_box.bounds.y,
                target_box.bounds.y + target_box.bounds.height,
            )
        };
        (
            LayoutPoint {
                x: source_center.x,
                y: source_y,
            },
            LayoutPoint {
                x: target_center.x,
                y: target_y,
            },
        )
    }
}

fn route_edge_points(
    source: LayoutPoint,
    target: LayoutPoint,
    horizontal_ranks: bool,
) -> Vec<LayoutPoint> {
    let epsilon = 0.001_f32;

    let points = if horizontal_ranks {
        if (source.y - target.y).abs() < epsilon {
            vec![source, target]
        } else {
            let mid_x = (source.x + target.x) / 2.0;
            vec![
                source,
                LayoutPoint {
                    x: mid_x,
                    y: source.y,
                },
                LayoutPoint {
                    x: mid_x,
                    y: target.y,
                },
                target,
            ]
        }
    } else if (source.x - target.x).abs() < epsilon {
        vec![source, target]
    } else {
        let mid_y = (source.y + target.y) / 2.0;
        vec![
            source,
            LayoutPoint {
                x: source.x,
                y: mid_y,
            },
            LayoutPoint {
                x: target.x,
                y: mid_y,
            },
            target,
        ]
    };

    simplify_polyline(points)
}

fn simplify_polyline(points: Vec<LayoutPoint>) -> Vec<LayoutPoint> {
    if points.len() <= 2 {
        return points;
    }

    let mut simplified = Vec::with_capacity(points.len());
    for point in points {
        if simplified.last() == Some(&point) {
            continue;
        }
        simplified.push(point);

        while simplified.len() >= 3 {
            let c = simplified[simplified.len() - 1];
            let b = simplified[simplified.len() - 2];
            let a = simplified[simplified.len() - 3];
            if is_axis_aligned_collinear(a, b, c) {
                simplified.remove(simplified.len() - 2);
            } else {
                break;
            }
        }
    }

    simplified
}

fn is_axis_aligned_collinear(a: LayoutPoint, b: LayoutPoint, c: LayoutPoint) -> bool {
    let epsilon = 0.001_f32;
    ((a.x - b.x).abs() < epsilon && (b.x - c.x).abs() < epsilon)
        || ((a.y - b.y).abs() < epsilon && (b.y - c.y).abs() < epsilon)
}

fn build_cluster_boxes(
    ir: &MermaidDiagramIr,
    nodes: &[LayoutNodeBox],
    spacing: LayoutSpacing,
) -> Vec<LayoutClusterBox> {
    ir.clusters
        .iter()
        .enumerate()
        .filter_map(|(cluster_index, cluster)| {
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;

            for member in &cluster.members {
                let Some(node_box) = nodes.get(member.0) else {
                    continue;
                };
                min_x = min_x.min(node_box.bounds.x);
                min_y = min_y.min(node_box.bounds.y);
                max_x = max_x.max(node_box.bounds.x + node_box.bounds.width);
                max_y = max_y.max(node_box.bounds.y + node_box.bounds.height);
            }

            (min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite())
                .then_some(LayoutClusterBox {
                    cluster_index,
                    bounds: LayoutRect {
                        x: min_x - spacing.cluster_padding,
                        y: min_y - spacing.cluster_padding,
                        width: (max_x - min_x) + (2.0 * spacing.cluster_padding),
                        height: (max_y - min_y) + (2.0 * spacing.cluster_padding),
                    },
                })
        })
        .collect()
}

fn compute_bounds(
    nodes: &[LayoutNodeBox],
    clusters: &[LayoutClusterBox],
    spacing: LayoutSpacing,
) -> LayoutRect {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for node in nodes {
        min_x = min_x.min(node.bounds.x);
        min_y = min_y.min(node.bounds.y);
        max_x = max_x.max(node.bounds.x + node.bounds.width);
        max_y = max_y.max(node.bounds.y + node.bounds.height);
    }

    for cluster in clusters {
        min_x = min_x.min(cluster.bounds.x);
        min_y = min_y.min(cluster.bounds.y);
        max_x = max_x.max(cluster.bounds.x + cluster.bounds.width);
        max_y = max_y.max(cluster.bounds.y + cluster.bounds.height);
    }

    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
    }

    LayoutRect {
        x: min_x - spacing.cluster_padding,
        y: min_y - spacing.cluster_padding,
        width: (max_x - min_x) + (2.0 * spacing.cluster_padding),
        height: (max_y - min_y) + (2.0 * spacing.cluster_padding),
    }
}

fn compute_edge_length_metrics(edges: &[LayoutEdgePath]) -> (f32, f32) {
    let mut total = 0.0_f32;
    let mut reversed_total = 0.0_f32;

    for edge in edges {
        let length = polyline_length(&edge.points);
        total += length;
        if edge.reversed {
            reversed_total += length;
        }
    }

    (total, reversed_total)
}

fn polyline_length(points: &[LayoutPoint]) -> f32 {
    points
        .windows(2)
        .map(|pair| {
            let dx = pair[1].x - pair[0].x;
            let dy = pair[1].y - pair[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

fn build_cycle_cluster_map(
    ir: &MermaidDiagramIr,
    cycle_result: &CycleRemovalResult,
) -> CycleClusterMap {
    let node_count = ir.nodes.len();
    let edges = resolved_edges(ir);
    let node_priority = stable_node_priorities(ir);
    let detection = detect_cycle_components(node_count, &edges, &node_priority);

    let mut node_representative = (0..node_count).collect::<Vec<_>>();
    let mut cluster_heads = BTreeSet::new();
    let mut cluster_members = BTreeMap::new();

    for component_index in &detection.cyclic_component_indexes {
        let Some(component_nodes) = detection.components.get(*component_index) else {
            continue;
        };
        if component_nodes.len() <= 1 {
            // Skip self-loops for cluster collapse — they're single nodes.
            continue;
        }

        // Choose the lowest-priority node as the representative (cluster head).
        let head = *component_nodes
            .iter()
            .min_by(|a, b| compare_priority(**a, **b, &node_priority))
            .unwrap_or(&component_nodes[0]);

        cluster_heads.insert(head);
        let mut members = component_nodes.clone();
        members.sort_by(|a, b| compare_priority(*a, *b, &node_priority));
        for &member in &members {
            node_representative[member] = head;
        }
        cluster_members.insert(head, members);
    }

    let _ = cycle_result; // Used for type coherence; detection is recomputed for isolation.

    CycleClusterMap {
        node_representative,
        cluster_heads,
        cluster_members,
    }
}

fn build_cycle_cluster_results(
    collapse_map: &CycleClusterMap,
    nodes: &mut [LayoutNodeBox],
    clusters: &mut Vec<LayoutClusterBox>,
    spacing: LayoutSpacing,
) -> Vec<LayoutCycleCluster> {
    let mut cycle_clusters = Vec::new();

    for (head, members) in &collapse_map.cluster_members {
        if members.len() <= 1 {
            continue;
        }

        // Find the head node's bounding box (copy values to satisfy borrow checker).
        let Some(head_box) = nodes.iter().find(|n| n.node_index == *head) else {
            continue;
        };
        let base_x = head_box.bounds.x;
        let base_y = head_box.bounds.y;
        let head_height = head_box.bounds.height;

        // Arrange member nodes (excluding head) in a compact grid within the cluster bounds.
        let non_head_members: Vec<usize> = members.iter().copied().filter(|m| m != head).collect();
        let member_count = non_head_members.len();
        let cols = ((member_count as f32).sqrt().ceil() as usize).max(1);

        let sub_spacing = spacing.node_spacing * 0.5;
        for (idx, &member_index) in non_head_members.iter().enumerate() {
            let col = idx % cols;
            let row = idx / cols;
            if let Some(member_box) = nodes.iter_mut().find(|n| n.node_index == member_index) {
                member_box.bounds.x =
                    base_x + (col as f32) * (member_box.bounds.width + sub_spacing);
                member_box.bounds.y = base_y
                    + head_height
                    + spacing.cluster_padding
                    + (row as f32) * (member_box.bounds.height + sub_spacing);
            }
        }

        // Compute the cluster bounding box over all members.
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for &member_index in members {
            if let Some(member_box) = nodes.iter().find(|n| n.node_index == member_index) {
                min_x = min_x.min(member_box.bounds.x);
                min_y = min_y.min(member_box.bounds.y);
                max_x = max_x.max(member_box.bounds.x + member_box.bounds.width);
                max_y = max_y.max(member_box.bounds.y + member_box.bounds.height);
            }
        }

        if min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite() {
            let cluster_bounds = LayoutRect {
                x: min_x - spacing.cluster_padding,
                y: min_y - spacing.cluster_padding,
                width: (max_x - min_x) + (2.0 * spacing.cluster_padding),
                height: (max_y - min_y) + (2.0 * spacing.cluster_padding),
            };

            cycle_clusters.push(LayoutCycleCluster {
                head_node_index: *head,
                member_node_indexes: members.clone(),
                bounds: cluster_bounds,
            });

            // Also add as a regular cluster box for rendering consistency.
            clusters.push(LayoutClusterBox {
                cluster_index: clusters.len(),
                bounds: cluster_bounds,
            });
        }
    }

    cycle_clusters
}

fn endpoint_node_index(ir: &MermaidDiagramIr, endpoint: IrEndpoint) -> Option<usize> {
    match endpoint {
        IrEndpoint::Node(node) => Some(node.0),
        IrEndpoint::Port(port) => ir.ports.get(port.0).map(|port_ref| port_ref.node.0),
        IrEndpoint::Unresolved => None,
    }
}

fn push_snapshot(
    trace: &mut LayoutTrace,
    stage: &'static str,
    node_count: usize,
    edge_count: usize,
    reversed_edges: usize,
    crossing_count: usize,
) {
    trace.snapshots.push(LayoutStageSnapshot {
        stage,
        reversed_edges,
        crossing_count,
        node_count,
        edge_count,
    });
}

#[must_use]
pub fn layout_stats_from(layout: &DiagramLayout) -> LayoutStats {
    layout.stats
}

#[cfg(test)]
mod tests {
    use super::{
        CycleStrategy, LayoutAlgorithm, LayoutPoint, layout, layout_diagram, layout_diagram_force,
        layout_diagram_force_traced, layout_diagram_traced, layout_diagram_with_cycle_strategy,
        route_edge_points,
    };
    use fm_core::{
        ArrowType, DiagramType, GraphDirection, IrCluster, IrClusterId, IrEdge, IrEndpoint,
        IrLabel, IrLabelId, IrNode, IrNodeId, MermaidDiagramIr,
    };

    fn sample_ir() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;
        ir.labels.push(IrLabel {
            text: "Start".to_string(),
            ..IrLabel::default()
        });
        ir.labels.push(IrLabel {
            text: "End".to_string(),
            ..IrLabel::default()
        });
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: Some(IrLabelId(0)),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            label: Some(IrLabelId(1)),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir
    }

    #[test]
    fn layout_reports_counts() {
        let ir = sample_ir();
        let stats = layout(&ir, LayoutAlgorithm::Auto);
        assert_eq!(stats.node_count, 2);
        assert_eq!(stats.edge_count, 1);
    }

    #[test]
    fn traced_layout_is_deterministic() {
        let ir = sample_ir();
        let first = layout_diagram_traced(&ir);
        let second = layout_diagram_traced(&ir);
        assert_eq!(first, second);
    }

    #[test]
    fn layout_contains_node_boxes_and_bounds() {
        let ir = sample_ir();
        let layout = layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 2);
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
    }

    #[test]
    fn crossing_count_reports_layer_crossings() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }

        // K2,2 across adjacent layers: at least one crossing remains regardless ordering.
        for (from, to) in [(0, 2), (0, 3), (1, 2), (1, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let stats = layout(&ir, LayoutAlgorithm::Auto);
        assert!(stats.crossing_count > 0);
    }

    #[test]
    fn cycle_removal_marks_reversed_edges_for_simple_cycle() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let stats = layout(&ir, LayoutAlgorithm::Auto);
        assert!(stats.reversed_edges >= 1);
    }

    #[test]
    fn cycle_aware_marks_back_edges_without_reversal() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::CycleAware);
        assert_eq!(layout.stats.reversed_edges, 0);
        assert_eq!(layout.stats.cycle_count, 1);
        assert_eq!(layout.stats.cycle_node_count, 3);
        assert_eq!(layout.stats.max_cycle_size, 3);
        assert!(layout.edges.iter().any(|edge| edge.reversed));
    }

    #[test]
    fn dfs_back_cycle_strategy_is_deterministic() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (2, 3), (3, 1)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let first = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::DfsBack);
        let second = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::DfsBack);
        assert_eq!(first, second);
        assert!(first.stats.reversed_edges >= 1);
        assert!(first.edges.iter().any(|edge| edge.reversed));
    }

    #[test]
    fn bt_direction_reverses_vertical_rank_axis() {
        let mut ir = sample_ir();
        ir.direction = GraphDirection::BT;

        let layout = layout_diagram(&ir);
        let a_node = layout.nodes.iter().find(|node| node.node_id == "A");
        let b_node = layout.nodes.iter().find(|node| node.node_id == "B");
        let (Some(a_node), Some(b_node)) = (a_node, b_node) else {
            panic!("expected A and B nodes in layout");
        };

        assert!(b_node.bounds.y < a_node.bounds.y);
    }

    #[test]
    fn rl_direction_reverses_horizontal_rank_axis() {
        let mut ir = sample_ir();
        ir.direction = GraphDirection::RL;

        let layout = layout_diagram(&ir);
        let a_node = layout.nodes.iter().find(|node| node.node_id == "A");
        let b_node = layout.nodes.iter().find(|node| node.node_id == "B");
        let (Some(a_node), Some(b_node)) = (a_node, b_node) else {
            panic!("expected A and B nodes in layout");
        };

        assert!(b_node.bounds.x < a_node.bounds.x);
    }

    #[test]
    fn vertical_routing_adds_turn_for_offset_nodes() {
        let points = route_edge_points(
            LayoutPoint { x: 10.0, y: 40.0 },
            LayoutPoint { x: 100.0, y: 120.0 },
            false,
        );
        assert_eq!(points.len(), 4);
        assert_eq!(
            points.first().copied(),
            Some(LayoutPoint { x: 10.0, y: 40.0 })
        );
        assert_eq!(
            points.last().copied(),
            Some(LayoutPoint { x: 100.0, y: 120.0 })
        );
    }

    #[test]
    fn horizontal_routing_adds_turn_for_offset_nodes() {
        let points = route_edge_points(
            LayoutPoint { x: 40.0, y: 10.0 },
            LayoutPoint { x: 120.0, y: 100.0 },
            true,
        );
        assert_eq!(points.len(), 4);
        assert_eq!(
            points.first().copied(),
            Some(LayoutPoint { x: 40.0, y: 10.0 })
        );
        assert_eq!(
            points.last().copied(),
            Some(LayoutPoint { x: 120.0, y: 100.0 })
        );
    }

    #[test]
    fn greedy_cycle_strategy_is_deterministic() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (2, 3), (3, 1)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let first = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::Greedy);
        let second = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::Greedy);
        assert_eq!(first, second);
        assert!(first.stats.reversed_edges >= 1);
        assert!(first.edges.iter().any(|edge| edge.reversed));
    }

    #[test]
    fn mfas_cycle_strategy_is_deterministic() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (2, 3), (3, 1)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let first = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::MfasApprox);
        let second = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::MfasApprox);
        assert_eq!(first, second);
        assert!(first.stats.reversed_edges >= 1);
    }

    #[test]
    fn greedy_breaks_simple_cycle() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::Greedy);
        assert!(layout.stats.reversed_edges >= 1);
        assert_eq!(layout.stats.cycle_count, 1);
        assert_eq!(layout.stats.cycle_node_count, 3);
    }

    #[test]
    fn mfas_breaks_simple_cycle() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::MfasApprox);
        assert!(layout.stats.reversed_edges >= 1);
        assert_eq!(layout.stats.cycle_count, 1);
    }

    #[test]
    fn self_loop_detected_as_cycle() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(0)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::DfsBack);
        assert_eq!(layout.stats.cycle_count, 1);
        assert_eq!(layout.stats.cycle_node_count, 1);
        assert_eq!(layout.stats.max_cycle_size, 1);
    }

    #[test]
    fn multiple_disconnected_cycles_detected() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        // Two separate triangles: A->B->C->A and D->E->F->D
        for node_id in ["A", "B", "C", "D", "E", "F"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::Greedy);
        assert_eq!(layout.stats.cycle_count, 2);
        assert_eq!(layout.stats.cycle_node_count, 6);
        assert_eq!(layout.stats.max_cycle_size, 3);
        assert!(layout.stats.reversed_edges >= 2);
    }

    #[test]
    fn nested_cycles_handled_correctly() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        // A->B->C->A forms inner cycle, A->B->C->D->A forms outer cycle sharing edges
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (2, 3), (3, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::DfsBack);
        // All 4 nodes form one SCC due to shared edges
        assert!(layout.stats.cycle_count >= 1);
        assert!(layout.stats.cycle_node_count >= 3);
        assert!(layout.stats.reversed_edges >= 1);
    }

    #[test]
    fn acyclic_graph_has_no_reversals() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (0, 2), (1, 3), (2, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        for strategy in [
            CycleStrategy::Greedy,
            CycleStrategy::DfsBack,
            CycleStrategy::MfasApprox,
            CycleStrategy::CycleAware,
        ] {
            let layout = layout_diagram_with_cycle_strategy(&ir, strategy);
            assert_eq!(
                layout.stats.reversed_edges, 0,
                "strategy {:?} should not reverse edges in acyclic graph",
                strategy
            );
            assert_eq!(layout.stats.cycle_count, 0);
            assert!(!layout.edges.iter().any(|e| e.reversed));
        }
    }

    #[test]
    fn all_strategies_produce_valid_layout_for_cyclic_graph() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        for strategy in [
            CycleStrategy::Greedy,
            CycleStrategy::DfsBack,
            CycleStrategy::MfasApprox,
            CycleStrategy::CycleAware,
        ] {
            let layout = layout_diagram_with_cycle_strategy(&ir, strategy);
            // All strategies should produce valid layout with 3 nodes and 3 edges
            assert_eq!(layout.nodes.len(), 3, "strategy {:?}", strategy);
            assert_eq!(layout.edges.len(), 3, "strategy {:?}", strategy);
            assert!(layout.bounds.width > 0.0, "strategy {:?}", strategy);
            assert!(layout.bounds.height > 0.0, "strategy {:?}", strategy);
            // All strategies should detect the cycle
            assert_eq!(layout.stats.cycle_count, 1, "strategy {:?}", strategy);
        }
    }

    #[test]
    fn cycle_strategy_parse_roundtrip() {
        for strategy in [
            CycleStrategy::Greedy,
            CycleStrategy::DfsBack,
            CycleStrategy::MfasApprox,
            CycleStrategy::CycleAware,
        ] {
            let parsed = CycleStrategy::parse(strategy.as_str());
            assert_eq!(
                parsed,
                Some(strategy),
                "roundtrip failed for {:?}",
                strategy
            );
        }
    }

    #[test]
    fn cycle_cluster_collapse_groups_scc_members() {
        use super::LayoutConfig;

        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        // Build: A->B->C->A (cycle) + D (separate node connected from A)
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (0, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let config = LayoutConfig {
            cycle_strategy: CycleStrategy::Greedy,
            collapse_cycle_clusters: true,
        };
        let layout = super::layout_diagram_with_config(&ir, config);

        // Should have one collapsed cluster (the A->B->C cycle)
        assert_eq!(layout.stats.collapsed_clusters, 1);
        assert_eq!(layout.cycle_clusters.len(), 1);

        let cluster = &layout.cycle_clusters[0];
        assert_eq!(cluster.member_node_indexes.len(), 3);
        assert!(cluster.bounds.width > 0.0);
        assert!(cluster.bounds.height > 0.0);

        // All 4 nodes should still be in the layout
        assert_eq!(layout.nodes.len(), 4);
    }

    #[test]
    fn edge_length_metrics_computed_for_cyclic_graph() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::Greedy);
        // Total edge length should be positive (3 edges)
        assert!(layout.stats.total_edge_length > 0.0);
        // At least one edge is reversed, so reversed_edge_total_length > 0
        assert!(layout.stats.reversed_edge_total_length > 0.0);
        // Reversed edge length should not exceed total
        assert!(layout.stats.reversed_edge_total_length <= layout.stats.total_edge_length);
    }

    #[test]
    fn edge_length_metrics_zero_for_acyclic_graph() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        assert!(layout.stats.total_edge_length > 0.0);
        assert!((layout.stats.reversed_edge_total_length - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cycle_cluster_collapse_disabled_produces_no_clusters() {
        use super::LayoutConfig;

        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let config = LayoutConfig {
            cycle_strategy: CycleStrategy::Greedy,
            collapse_cycle_clusters: false,
        };
        let layout = super::layout_diagram_with_config(&ir, config);

        assert_eq!(layout.stats.collapsed_clusters, 0);
        assert!(layout.cycle_clusters.is_empty());
    }

    #[test]
    fn cycle_strategy_parse_aliases() {
        assert_eq!(CycleStrategy::parse("dfs"), Some(CycleStrategy::DfsBack));
        assert_eq!(
            CycleStrategy::parse("dfs_back"),
            Some(CycleStrategy::DfsBack)
        );
        assert_eq!(
            CycleStrategy::parse("minimum-feedback-arc-set"),
            Some(CycleStrategy::MfasApprox)
        );
        assert_eq!(
            CycleStrategy::parse("cycleaware"),
            Some(CycleStrategy::CycleAware)
        );
        assert_eq!(CycleStrategy::parse("unknown"), None);
    }

    #[test]
    fn lr_same_rank_nodes_with_different_widths_share_column_position() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;

        for text in [
            "root",
            "narrow",
            "this target label is intentionally much wider",
        ] {
            ir.labels.push(IrLabel {
                text: text.to_string(),
                ..IrLabel::default()
            });
        }

        for (node_id, label_id) in [("R", 0), ("A", 1), ("B", 2)] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                label: Some(IrLabelId(label_id)),
                ..IrNode::default()
            });
        }

        for (from, to) in [(0, 1), (0, 2)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        let a_node = layout.nodes.iter().find(|node| node.node_id == "A");
        let b_node = layout.nodes.iter().find(|node| node.node_id == "B");
        let (Some(a_node), Some(b_node)) = (a_node, b_node) else {
            panic!("expected A and B nodes in layout");
        };

        assert!((a_node.bounds.x - b_node.bounds.x).abs() < 0.001);
    }

    #[test]
    fn tb_disconnected_components_do_not_collapse_into_horizontal_strip() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;

        // 20 disconnected 2-node chains (A_i -> B_i).
        for index in 0..20 {
            ir.nodes.push(IrNode {
                id: format!("A{index}"),
                ..IrNode::default()
            });
            ir.nodes.push(IrNode {
                id: format!("B{index}"),
                ..IrNode::default()
            });
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(index * 2)),
                to: IrEndpoint::Node(IrNodeId(index * 2 + 1)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 40);
        assert_eq!(layout.edges.len(), 20);
        assert!(
            layout.bounds.width < layout.bounds.height * 2.0,
            "expected stacked components in TB layout, got width={} height={}",
            layout.bounds.width,
            layout.bounds.height,
        );
    }

    #[test]
    fn tb_isolated_nodes_remain_in_a_single_rank_band() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;

        for index in 0..6 {
            ir.nodes.push(IrNode {
                id: format!("N{index}"),
                ..IrNode::default()
            });
        }

        let layout = layout_diagram(&ir);
        let distinct_ranks: std::collections::BTreeSet<usize> =
            layout.nodes.iter().map(|node| node.rank).collect();
        assert_eq!(
            distinct_ranks.len(),
            1,
            "isolated nodes should stay in a shared rank band, got ranks {distinct_ranks:?}"
        );
    }

    #[test]
    fn tb_mixed_components_keep_isolates_outside_connected_rank_bands() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;

        for index in 0..5 {
            ir.nodes.push(IrNode {
                id: format!("A{index}"),
                ..IrNode::default()
            });
            ir.nodes.push(IrNode {
                id: format!("B{index}"),
                ..IrNode::default()
            });
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(index * 2)),
                to: IrEndpoint::Node(IrNodeId(index * 2 + 1)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        for index in 0..10 {
            ir.nodes.push(IrNode {
                id: format!("Iso{index}"),
                ..IrNode::default()
            });
        }

        let layout = layout_diagram(&ir);
        let mut connected_ranks = std::collections::BTreeSet::new();
        let mut isolated_ranks = std::collections::BTreeSet::new();

        for node in &layout.nodes {
            if node.node_id.starts_with("Iso") {
                isolated_ranks.insert(node.rank);
            } else {
                connected_ranks.insert(node.rank);
            }
        }

        assert_eq!(
            isolated_ranks.len(),
            1,
            "all isolated nodes should share one rank band, got {isolated_ranks:?}"
        );
        assert!(
            connected_ranks.is_disjoint(&isolated_ranks),
            "isolated and connected nodes should not share rank bands; connected={connected_ranks:?} isolated={isolated_ranks:?}"
        );
    }

    // --- Force-directed layout tests ---

    fn sample_er_ir() -> MermaidDiagramIr {
        // ER-like diagram: no clear hierarchy, many-to-many relationships.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        for label in ["Users", "Orders", "Products", "Reviews"] {
            ir.labels.push(IrLabel {
                text: label.to_string(),
                ..IrLabel::default()
            });
        }
        for (i, node_id) in ["users", "orders", "products", "reviews"]
            .iter()
            .enumerate()
        {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                label: Some(IrLabelId(i)),
                ..IrNode::default()
            });
        }
        // Many-to-many: users <-> orders, orders <-> products, users <-> reviews, products <-> reviews
        for (from, to) in [(0, 1), (1, 2), (0, 3), (2, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Line,
                ..IrEdge::default()
            });
        }
        ir
    }

    #[test]
    fn force_layout_produces_valid_output() {
        let ir = sample_er_ir();
        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 4);
        assert_eq!(layout.edges.len(), 4);
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
    }

    #[test]
    fn force_layout_is_deterministic() {
        let ir = sample_er_ir();
        let first = layout_diagram_force_traced(&ir);
        let second = layout_diagram_force_traced(&ir);
        assert_eq!(first, second, "Force layout must be deterministic");
    }

    #[test]
    fn force_layout_no_node_overlap() {
        let ir = sample_er_ir();
        let layout = layout_diagram_force(&ir);
        for (i, a) in layout.nodes.iter().enumerate() {
            for b in layout.nodes.iter().skip(i + 1) {
                let overlap_x = (a.bounds.width + b.bounds.width) / 2.0
                    - ((a.bounds.x + a.bounds.width / 2.0) - (b.bounds.x + b.bounds.width / 2.0))
                        .abs();
                let overlap_y = (a.bounds.height + b.bounds.height) / 2.0
                    - ((a.bounds.y + a.bounds.height / 2.0) - (b.bounds.y + b.bounds.height / 2.0))
                        .abs();
                assert!(
                    overlap_x <= 1.0 || overlap_y <= 1.0,
                    "Nodes {} and {} overlap: overlap_x={overlap_x}, overlap_y={overlap_y}",
                    a.node_id,
                    b.node_id,
                );
            }
        }
    }

    #[test]
    fn force_layout_empty_graph() {
        let ir = MermaidDiagramIr::empty(DiagramType::Er);
        let layout = layout_diagram_force(&ir);
        assert!(layout.nodes.is_empty());
        assert!(layout.edges.is_empty());
        assert_eq!(layout.stats.node_count, 0);
    }

    #[test]
    fn force_layout_single_node() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 1);
        assert!(layout.nodes[0].bounds.width > 0.0);
        assert!(layout.nodes[0].bounds.height > 0.0);
        assert!(layout.nodes[0].bounds.x >= 0.0);
        assert!(layout.nodes[0].bounds.y >= 0.0);
    }

    #[test]
    fn force_layout_disconnected_components() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        // Two disconnected pairs: A-B and C-D
        for (from, to) in [(0, 1), (2, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Line,
                ..IrEdge::default()
            });
        }
        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 4);
        assert_eq!(layout.edges.len(), 2);
        // All positions should be non-negative.
        for node in &layout.nodes {
            assert!(node.bounds.x >= 0.0, "node {} has negative x", node.node_id);
            assert!(node.bounds.y >= 0.0, "node {} has negative y", node.node_id);
        }
    }

    #[test]
    fn force_layout_self_loop() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        // Self-loop edge should be skipped (not cause crash).
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(0)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 1);
        // Self-loop creates a degenerate edge (from == to node), still present in output.
        assert_eq!(layout.edges.len(), 1);
    }

    #[test]
    fn force_layout_connected_nodes_closer_than_disconnected() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        // Only A-B connected, C is isolated.
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Line,
            ..IrEdge::default()
        });

        let layout = layout_diagram_force(&ir);
        let a = layout.nodes.iter().find(|n| n.node_id == "A").unwrap();
        let b = layout.nodes.iter().find(|n| n.node_id == "B").unwrap();
        let c = layout.nodes.iter().find(|n| n.node_id == "C").unwrap();

        let a_center = a.bounds.center();
        let b_center = b.bounds.center();
        let c_center = c.bounds.center();

        let dist_ab =
            ((a_center.x - b_center.x).powi(2) + (a_center.y - b_center.y).powi(2)).sqrt();
        let dist_ac =
            ((a_center.x - c_center.x).powi(2) + (a_center.y - c_center.y).powi(2)).sqrt();

        // Connected nodes should generally be closer than disconnected.
        assert!(
            dist_ab < dist_ac * 1.5,
            "Connected A-B distance ({dist_ab}) should be less than A-C distance ({dist_ac})"
        );
    }

    #[test]
    fn force_layout_with_clusters() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Line,
            ..IrEdge::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(2)),
            to: IrEndpoint::Node(IrNodeId(3)),
            arrow: ArrowType::Line,
            ..IrEdge::default()
        });
        // Cluster 0: A, B. Cluster 1: C, D.
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0), IrNodeId(1)],
            span: fm_core::Span::default(),
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(1),
            title: None,
            members: vec![IrNodeId(2), IrNodeId(3)],
            span: fm_core::Span::default(),
        });

        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 4);
        assert_eq!(layout.clusters.len(), 2);
        // Cluster bounds should be non-zero.
        for cluster in &layout.clusters {
            assert!(cluster.bounds.width > 0.0);
            assert!(cluster.bounds.height > 0.0);
        }
    }

    #[test]
    fn force_layout_edges_have_valid_points() {
        let ir = sample_er_ir();
        let layout = layout_diagram_force(&ir);
        for edge in &layout.edges {
            assert!(
                edge.points.len() >= 2,
                "Edge {} should have at least 2 points",
                edge.edge_index
            );
            for pt in &edge.points {
                assert!(pt.x.is_finite(), "Edge point x must be finite");
                assert!(pt.y.is_finite(), "Edge point y must be finite");
            }
        }
    }

    #[test]
    fn force_layout_edge_lengths_computed() {
        let ir = sample_er_ir();
        let layout = layout_diagram_force(&ir);
        assert!(layout.stats.total_edge_length > 0.0);
        // Force layout has no reversed edges.
        assert!((layout.stats.reversed_edge_total_length - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn force_layout_larger_graph() {
        // 20-node graph to verify it handles larger inputs.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        for i in 0..20 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                ..IrNode::default()
            });
        }
        // Ring topology + cross links.
        for i in 0..20 {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(i)),
                to: IrEndpoint::Node(IrNodeId((i + 1) % 20)),
                arrow: ArrowType::Line,
                ..IrEdge::default()
            });
        }
        // A few cross links.
        for (from, to) in [(0, 10), (5, 15), (3, 17)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Line,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 20);
        assert_eq!(layout.edges.len(), 23);
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
        assert!(layout.stats.total_edge_length > 0.0);
    }

    #[test]
    fn force_layout_dispatch_via_algorithm_enum() {
        let ir = sample_er_ir();
        let stats = layout(&ir, LayoutAlgorithm::Force);
        assert_eq!(stats.node_count, 4);
        assert_eq!(stats.edge_count, 4);
    }

    #[test]
    fn force_layout_trace_has_stages() {
        let ir = sample_er_ir();
        let traced = layout_diagram_force_traced(&ir);
        assert!(
            traced.trace.snapshots.len() >= 3,
            "Expected at least 3 trace stages: init, simulation, overlap_removal"
        );
        let stage_names: Vec<&str> = traced.trace.snapshots.iter().map(|s| s.stage).collect();
        assert!(stage_names.contains(&"force_init"));
        assert!(stage_names.contains(&"force_simulation"));
        assert!(stage_names.contains(&"force_overlap_removal"));
    }

    #[test]
    fn force_layout_all_positions_nonnegative() {
        let ir = sample_er_ir();
        let layout = layout_diagram_force(&ir);
        for node in &layout.nodes {
            assert!(
                node.bounds.x >= 0.0,
                "Node {} x={} is negative",
                node.node_id,
                node.bounds.x
            );
            assert!(
                node.bounds.y >= 0.0,
                "Node {} y={} is negative",
                node.node_id,
                node.bounds.y
            );
        }
    }

    // --- Crossing refinement tests ---

    #[test]
    fn refinement_improves_or_maintains_crossings() {
        // K2,2: A->C, A->D, B->C, B->D — barycenter may not find optimal.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 2), (0, 3), (1, 2), (1, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        // Refinement should never increase crossings over barycenter result.
        assert!(
            layout.stats.crossing_count <= layout.stats.crossing_count_before_refinement,
            "Refinement should not increase crossings: before={}, after={}",
            layout.stats.crossing_count_before_refinement,
            layout.stats.crossing_count,
        );
    }

    #[test]
    fn refinement_handles_zero_crossings() {
        // Linear chain: A->B->C — zero crossings, refinement should be a no-op.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        assert_eq!(layout.stats.crossing_count, 0);
        assert_eq!(layout.stats.crossing_count_before_refinement, 0);
    }

    #[test]
    fn refinement_is_deterministic() {
        // Dense graph where refinement has room to work.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D", "E", "F"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        // Layer 1: A, B, C. Layer 2: D, E, F. Cross-connected.
        for (from, to) in [(0, 3), (0, 5), (1, 4), (1, 3), (2, 5), (2, 4)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let first = layout_diagram(&ir);
        let second = layout_diagram(&ir);
        assert_eq!(first.stats.crossing_count, second.stats.crossing_count);
        assert_eq!(first, second);
    }

    #[test]
    fn refinement_tracks_before_after_stats() {
        // Graph where refinement might improve crossings.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D", "E"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 2), (0, 3), (0, 4), (1, 2), (1, 4)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        // Before refinement count is recorded.
        assert!(
            layout.stats.crossing_count_before_refinement >= layout.stats.crossing_count,
            "Before should be >= after: before={}, after={}",
            layout.stats.crossing_count_before_refinement,
            layout.stats.crossing_count,
        );
    }

    #[test]
    fn refinement_preserves_layout_validity() {
        // Dense crossing graph.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for i in 0..8 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                ..IrNode::default()
            });
        }
        // 4-source to 4-target with cross connections.
        for from in 0..4 {
            for to in 4..8 {
                ir.edges.push(IrEdge {
                    from: IrEndpoint::Node(IrNodeId(from)),
                    to: IrEndpoint::Node(IrNodeId(to)),
                    arrow: ArrowType::Arrow,
                    ..IrEdge::default()
                });
            }
        }

        let layout = layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 8);
        assert_eq!(layout.edges.len(), 16);
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
        // All nodes should have positive dimensions.
        for node in &layout.nodes {
            assert!(node.bounds.width > 0.0);
            assert!(node.bounds.height > 0.0);
        }
    }

    #[test]
    fn trace_includes_refinement_stage() {
        let ir = sample_ir();
        let traced = layout_diagram_traced(&ir);
        let stage_names: Vec<&str> = traced.trace.snapshots.iter().map(|s| s.stage).collect();
        assert!(
            stage_names.contains(&"crossing_refinement"),
            "Trace should include crossing_refinement stage, got: {stage_names:?}"
        );
    }
}
