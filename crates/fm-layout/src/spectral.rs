//! Spectral graph partitioning for hierarchical layout decomposition.
//!
//! Uses eigendecomposition of the graph Laplacian to partition large diagrams into
//! well-separated subgraphs. The Fiedler vector (eigenvector of the second-smallest
//! eigenvalue) provides the optimal bisection that minimizes the normalized cut.
//!
//! # Algorithm
//!
//! 1. Build the graph Laplacian `L = D - A` (degree matrix minus adjacency matrix).
//! 2. Compute the Fiedler vector (eigenvector corresponding to λ₂).
//! 3. Bisect: nodes with Fiedler value < median go to partition 0, rest to partition 1.
//! 4. Recurse for k-way partitioning.
//!
//! # References
//!
//! - Fiedler, "Algebraic Connectivity of Graphs" (Czech Math Journal 1973)
//! - Shi & Malik, "Normalized Cuts and Image Segmentation" (IEEE TPAMI 2000)
//! - Von Luxburg, "A Tutorial on Spectral Clustering" (Statistics and Computing 2007)

use std::collections::BTreeSet;

use fm_core::{IrEndpoint, MermaidDiagramIr};
use nalgebra::{DMatrix, DVector, SymmetricEigen};
use tracing::{debug, trace};

/// Resolve an `IrEndpoint` to a node index, handling ports.
fn endpoint_index(ir: &MermaidDiagramIr, endpoint: IrEndpoint) -> Option<usize> {
    match endpoint {
        IrEndpoint::Node(node) => {
            if node.0 < ir.nodes.len() {
                Some(node.0)
            } else {
                None
            }
        }
        IrEndpoint::Port(port) => {
            let node_idx = ir.ports.get(port.0).map(|port_ref| port_ref.node.0)?;
            if node_idx < ir.nodes.len() {
                Some(node_idx)
            } else {
                None
            }
        }
        IrEndpoint::Unresolved => None,
    }
}

/// A partition of graph nodes produced by spectral bisection.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectralPartition {
    /// Node indices belonging to this partition (sorted).
    pub node_indices: Vec<usize>,
    /// The partition's share of total graph volume (sum of degrees).
    pub volume_fraction: f64,
}

/// Result of spectral graph partitioning.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectralPartitionResult {
    /// The partitions produced.
    pub partitions: Vec<SpectralPartition>,
    /// The Fiedler value (algebraic connectivity λ₂).
    pub fiedler_value: f64,
    /// Number of edges crossing partition boundaries.
    pub cut_edges: usize,
    /// Normalized cut value: `cut(S₁,S₂) * (1/vol(S₁) + 1/vol(S₂))`.
    pub normalized_cut: f64,
    /// Whether the graph was too small or disconnected to meaningfully partition.
    pub trivial: bool,
}

/// Configuration for spectral partitioning.
#[derive(Debug, Clone, Copy)]
pub struct SpectralConfig {
    /// Minimum number of nodes before partitioning activates.
    pub min_nodes: usize,
    /// Target number of partitions (must be a power of 2 for recursive bisection).
    pub target_partitions: usize,
    /// Minimum balance ratio for each partition (0.0–0.5). Default 0.2.
    /// A partition must contain at least `balance_threshold * n` nodes.
    pub balance_threshold: f64,
    /// Maximum recursion depth for k-way partitioning.
    pub max_depth: usize,
}

impl Default for SpectralConfig {
    fn default() -> Self {
        Self {
            min_nodes: 200,
            target_partitions: 4,
            balance_threshold: 0.2,
            max_depth: 4,
        }
    }
}

/// Build the graph Laplacian matrix `L = D - A` from the diagram IR.
///
/// The adjacency matrix is constructed from the IR's edge list, treating
/// edges as undirected (both directions contribute). Self-loops are ignored.
/// The degree matrix `D` is diagonal with `D[i][i] = degree(i)`.
fn build_laplacian(ir: &MermaidDiagramIr) -> DMatrix<f64> {
    let n = ir.nodes.len();
    let mut laplacian = DMatrix::zeros(n, n);

    for edge in &ir.edges {
        let Some(from) = endpoint_index(ir, edge.from) else {
            continue;
        };
        let Some(to) = endpoint_index(ir, edge.to) else {
            continue;
        };
        if from != to {
            // Undirected: add weight 1.0 in both directions
            laplacian[(from, to)] -= 1.0;
            laplacian[(to, from)] -= 1.0;
            laplacian[(from, from)] += 1.0;
            laplacian[(to, to)] += 1.0;
        }
    }

    laplacian
}

/// Compute the Fiedler vector (eigenvector of second-smallest eigenvalue of L).
///
/// Returns `(fiedler_value, fiedler_vector)` or `None` if the graph is
/// disconnected (λ₂ ≈ 0) or too small.
fn compute_fiedler_vector(laplacian: &DMatrix<f64>) -> Option<(f64, DVector<f64>)> {
    let n = laplacian.nrows();
    if n < 2 {
        return None;
    }

    let eigen = SymmetricEigen::new(laplacian.clone());

    // Eigenvalues from SymmetricEigen are not guaranteed sorted.
    // Find indices sorted by eigenvalue ascending.
    let mut indexed_eigenvalues: Vec<(usize, f64)> = eigen
        .eigenvalues
        .iter()
        .enumerate()
        .map(|(i, &v)| (i, v))
        .collect();
    indexed_eigenvalues.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // λ₁ should be ≈ 0 (constant eigenvector). λ₂ is the Fiedler value.
    if indexed_eigenvalues.len() < 2 {
        return None;
    }

    let (fiedler_idx, fiedler_value) = indexed_eigenvalues[1];

    // If Fiedler value is near zero, graph is disconnected.
    // The eigenvector corresponding to this zero eigenvalue will be piece-wise constant
    // and perfectly separate the components when bisected!
    if fiedler_value < 1e-10 {
        debug!(
            fiedler_value,
            "Fiedler value near zero — graph disconnected, vector will separate components."
        );
    }

    // Guard against numerical instability producing NaN.
    if fiedler_value.is_nan() {
        debug!("Fiedler value is NaN — numerical instability, skipping spectral partition");
        return None;
    }

    let fiedler_vector = eigen.eigenvectors.column(fiedler_idx).into_owned();

    // Check for NaN in the vector itself.
    if fiedler_vector.iter().any(|v| v.is_nan()) {
        debug!("Fiedler vector contains NaN — numerical instability, skipping");
        return None;
    }

    trace!(
        fiedler_value,
        vector_len = fiedler_vector.len(),
        "Computed Fiedler vector"
    );

    Some((fiedler_value, fiedler_vector))
}

/// Bisect a set of node indices using the Fiedler vector.
///
/// Nodes with Fiedler component below the median go to partition 0,
/// the rest go to partition 1. The `balance_threshold` ensures neither
/// partition is smaller than `balance_threshold * n`.
fn spectral_bisect(
    node_indices: &[usize],
    fiedler_vector: &DVector<f64>,
    balance_threshold: f64,
) -> (Vec<usize>, Vec<usize>) {
    let n = node_indices.len();
    if n < 2 {
        return (node_indices.to_vec(), Vec::new());
    }

    // Collect (node_index, fiedler_component) pairs and sort by fiedler value.
    let mut indexed: Vec<(usize, f64)> = node_indices
        .iter()
        .map(|&idx| (idx, fiedler_vector[idx]))
        .collect();
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Compute split point respecting balance constraint.
    let min_size = ((n as f64) * balance_threshold).ceil() as usize;
    let min_size = min_size.max(1);
    let upper_bound = n.saturating_sub(min_size);
    // If the balance constraint is unsatisfiable (min_size > n/2), fall back to
    // a simple midpoint split — both halves are as balanced as possible.
    let split = if min_size > upper_bound {
        n / 2
    } else {
        (n / 2).clamp(min_size, upper_bound)
    };

    let part0: Vec<usize> = indexed[..split].iter().map(|&(idx, _)| idx).collect();
    let part1: Vec<usize> = indexed[split..].iter().map(|&(idx, _)| idx).collect();

    trace!(
        part0_size = part0.len(),
        part1_size = part1.len(),
        split_point = split,
        "Spectral bisection complete"
    );

    (part0, part1)
}

/// Compute the volume (sum of degrees) for a set of nodes in the IR.
fn compute_volume(ir: &MermaidDiagramIr, node_indices: &[usize]) -> f64 {
    let node_set: BTreeSet<usize> = node_indices.iter().copied().collect();
    let mut volume = 0.0_f64;
    for edge in &ir.edges {
        let Some(from) = endpoint_index(ir, edge.from) else {
            continue;
        };
        let Some(to) = endpoint_index(ir, edge.to) else {
            continue;
        };
        if node_set.contains(&from) {
            volume += 1.0;
        }
        if node_set.contains(&to) {
            volume += 1.0;
        }
    }
    volume
}

/// Count edges crossing between two partitions.
fn count_cut_edges(ir: &MermaidDiagramIr, part0: &[usize], part1: &[usize]) -> usize {
    let set0: BTreeSet<usize> = part0.iter().copied().collect();
    let set1: BTreeSet<usize> = part1.iter().copied().collect();
    let mut cut = 0;
    for edge in &ir.edges {
        let Some(from) = endpoint_index(ir, edge.from) else {
            continue;
        };
        let Some(to) = endpoint_index(ir, edge.to) else {
            continue;
        };
        if (set0.contains(&from) && set1.contains(&to))
            || (set1.contains(&from) && set0.contains(&to))
        {
            cut += 1;
        }
    }
    cut
}

/// Perform spectral bisection on the entire graph.
///
/// Returns a `SpectralPartitionResult` with two partitions, or a trivial
/// result if the graph is too small or disconnected.
pub fn spectral_bisect_graph(ir: &MermaidDiagramIr) -> SpectralPartitionResult {
    spectral_bisect_graph_with_config(ir, &SpectralConfig::default())
}

/// Perform spectral bisection with a custom configuration.
pub fn spectral_bisect_graph_with_config(
    ir: &MermaidDiagramIr,
    config: &SpectralConfig,
) -> SpectralPartitionResult {
    let n = ir.nodes.len();

    if n < 2 {
        return SpectralPartitionResult {
            partitions: vec![SpectralPartition {
                node_indices: (0..n).collect(),
                volume_fraction: 1.0,
            }],
            fiedler_value: 0.0,
            cut_edges: 0,
            normalized_cut: 0.0,
            trivial: true,
        };
    }

    debug!(
        node_count = n,
        "Building graph Laplacian for spectral partitioning"
    );
    let laplacian = build_laplacian(ir);

    let Some((fiedler_value, fiedler_vector)) = compute_fiedler_vector(&laplacian) else {
        // Disconnected or degenerate — return single partition
        return SpectralPartitionResult {
            partitions: vec![SpectralPartition {
                node_indices: (0..n).collect(),
                volume_fraction: 1.0,
            }],
            fiedler_value: 0.0,
            cut_edges: 0,
            normalized_cut: 0.0,
            trivial: true,
        };
    };

    let all_nodes: Vec<usize> = (0..n).collect();
    let (part0, part1) = spectral_bisect(&all_nodes, &fiedler_vector, config.balance_threshold);

    let total_volume = compute_volume(ir, &all_nodes);
    let vol0 = compute_volume(ir, &part0);
    let vol1 = compute_volume(ir, &part1);
    let cut_edges = count_cut_edges(ir, &part0, &part1);

    let normalized_cut = if vol0 > 0.0 && vol1 > 0.0 {
        (cut_edges as f64) * (1.0 / vol0 + 1.0 / vol1)
    } else {
        0.0
    };

    let vf0 = if total_volume > 0.0 {
        vol0 / total_volume
    } else {
        part0.len() as f64 / n as f64
    };
    let vf1 = if total_volume > 0.0 {
        vol1 / total_volume
    } else {
        part1.len() as f64 / n as f64
    };

    debug!(
        fiedler_value,
        cut_edges,
        normalized_cut,
        part0_size = part0.len(),
        part1_size = part1.len(),
        "Spectral bisection result"
    );

    SpectralPartitionResult {
        partitions: vec![
            SpectralPartition {
                node_indices: part0,
                volume_fraction: vf0,
            },
            SpectralPartition {
                node_indices: part1,
                volume_fraction: vf1,
            },
        ],
        fiedler_value,
        cut_edges,
        normalized_cut,
        trivial: false,
    }
}

/// Perform recursive spectral k-way partitioning.
///
/// Recursively bisects the graph until the target number of partitions is
/// reached or the recursion depth limit is hit.
pub fn spectral_partition_kway(
    ir: &MermaidDiagramIr,
    config: &SpectralConfig,
) -> SpectralPartitionResult {
    let n = ir.nodes.len();

    if n < 2 || config.target_partitions <= 1 {
        return spectral_bisect_graph_with_config(ir, config);
    }

    debug!(
        node_count = n,
        target_partitions = config.target_partitions,
        "Starting k-way spectral partitioning"
    );

    let laplacian = build_laplacian(ir);

    let Some((fiedler_value, _fiedler_vector)) = compute_fiedler_vector(&laplacian) else {
        return SpectralPartitionResult {
            partitions: vec![SpectralPartition {
                node_indices: (0..n).collect(),
                volume_fraction: 1.0,
            }],
            fiedler_value: 0.0,
            cut_edges: 0,
            normalized_cut: 0.0,
            trivial: true,
        };
    };

    // Start with all nodes, recursively bisect.
    // Each sub-partition computes its own Fiedler vector from the sub-Laplacian.
    let all_nodes: Vec<usize> = (0..n).collect();
    let mut work_queue: Vec<Vec<usize>> = vec![all_nodes];
    let mut depth = 0;

    while work_queue.len() < config.target_partitions && depth < config.max_depth {
        let mut next_queue = Vec::new();
        for partition_nodes in &work_queue {
            if partition_nodes.len() < 4 {
                // Too small to bisect meaningfully
                next_queue.push(partition_nodes.clone());
                continue;
            }

            // Build a sub-Laplacian for this partition and compute its Fiedler vector
            let sub_n = partition_nodes.len();
            let mut sub_laplacian = DMatrix::zeros(sub_n, sub_n);

            // Map global → local indices
            let global_to_local: std::collections::BTreeMap<usize, usize> = partition_nodes
                .iter()
                .enumerate()
                .map(|(local, &global)| (global, local))
                .collect();

            for edge in &ir.edges {
                let Some(from_global) = endpoint_index(ir, edge.from) else {
                    continue;
                };
                let Some(to_global) = endpoint_index(ir, edge.to) else {
                    continue;
                };
                if from_global == to_global {
                    continue;
                }
                if let (Some(&from_local), Some(&to_local)) = (
                    global_to_local.get(&from_global),
                    global_to_local.get(&to_global),
                ) {
                    sub_laplacian[(from_local, to_local)] -= 1.0;
                    sub_laplacian[(to_local, from_local)] -= 1.0;
                    sub_laplacian[(from_local, from_local)] += 1.0;
                    sub_laplacian[(to_local, to_local)] += 1.0;
                }
            }

            if let Some((_sub_fiedler_val, sub_fiedler_vec)) =
                compute_fiedler_vector(&sub_laplacian)
            {
                // Bisect using sub-Fiedler vector, but map back to global indices
                let local_indices: Vec<usize> = (0..sub_n).collect();
                let (local_part0, local_part1) =
                    spectral_bisect(&local_indices, &sub_fiedler_vec, config.balance_threshold);

                let global_part0: Vec<usize> =
                    local_part0.iter().map(|&l| partition_nodes[l]).collect();
                let global_part1: Vec<usize> =
                    local_part1.iter().map(|&l| partition_nodes[l]).collect();

                if !global_part0.is_empty() {
                    next_queue.push(global_part0);
                }
                if !global_part1.is_empty() {
                    next_queue.push(global_part1);
                }
            } else {
                // Can't bisect further (disconnected sub-partition)
                next_queue.push(partition_nodes.clone());
            }
        }
        work_queue = next_queue;
        depth += 1;
    }

    // Compute partition metrics
    let total_volume = compute_volume(ir, &(0..n).collect::<Vec<_>>());
    let mut total_cut_edges = 0_usize;
    let all_partitions_set: Vec<BTreeSet<usize>> = work_queue
        .iter()
        .map(|p| p.iter().copied().collect())
        .collect();

    // Count edges crossing any partition boundary
    for edge in &ir.edges {
        let Some(from) = endpoint_index(ir, edge.from) else {
            continue;
        };
        let Some(to) = endpoint_index(ir, edge.to) else {
            continue;
        };
        if from != to {
            let from_part = all_partitions_set.iter().position(|s| s.contains(&from));
            let to_part = all_partitions_set.iter().position(|s| s.contains(&to));
            if from_part != to_part {
                total_cut_edges += 1;
            }
        }
    }

    let partitions: Vec<SpectralPartition> = work_queue
        .into_iter()
        .map(|node_indices| {
            let vol = compute_volume(ir, &node_indices);
            SpectralPartition {
                volume_fraction: if total_volume > 0.0 {
                    vol / total_volume
                } else {
                    node_indices.len() as f64 / n as f64
                },
                node_indices,
            }
        })
        .collect();

    // Approximate normalized cut for k-way partition
    let normalized_cut = if partitions.len() >= 2 {
        let inv_vol_sum: f64 = partitions
            .iter()
            .map(|p| {
                let vol = compute_volume(ir, &p.node_indices);
                if vol > 0.0 { 1.0 / vol } else { 0.0 }
            })
            .sum();
        (total_cut_edges as f64) * inv_vol_sum
    } else {
        0.0
    };

    debug!(
        num_partitions = partitions.len(),
        total_cut_edges, normalized_cut, depth, "K-way spectral partitioning complete"
    );

    SpectralPartitionResult {
        partitions,
        fiedler_value,
        cut_edges: total_cut_edges,
        normalized_cut,
        trivial: false,
    }
}

/// Check whether a diagram IR is large enough to benefit from spectral partitioning.
#[must_use]
pub fn should_partition(ir: &MermaidDiagramIr, config: &SpectralConfig) -> bool {
    ir.nodes.len() >= config.min_nodes
}

/// Mapping from global node indices to partition-local indices.
#[derive(Debug, Clone)]
pub struct PartitionMapping {
    /// For each partition: a vec of global node indices in that partition.
    pub partitions: Vec<Vec<usize>>,
    /// Maps global node index → (partition_id, local_index_within_partition).
    pub global_to_partition: Vec<(usize, usize)>,
}

/// Build a `PartitionMapping` from a `SpectralPartitionResult`.
///
/// This mapping allows the layout pipeline to extract sub-IRs for each partition,
/// layout them independently, and then stitch the results together.
#[must_use]
pub fn build_partition_mapping(result: &SpectralPartitionResult, n: usize) -> PartitionMapping {
    let mut global_to_partition = vec![(0_usize, 0_usize); n];

    for (part_id, partition) in result.partitions.iter().enumerate() {
        for (local_idx, &global_idx) in partition.node_indices.iter().enumerate() {
            if global_idx < n {
                global_to_partition[global_idx] = (part_id, local_idx);
            }
        }
    }

    PartitionMapping {
        partitions: result
            .partitions
            .iter()
            .map(|p| p.node_indices.clone())
            .collect(),
        global_to_partition,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{
        ArrowType, DiagramType, IrEdge, IrEndpoint, IrLabel, IrLabelId, IrNode, IrNodeId,
        MermaidDiagramIr, Span,
    };

    /// Helper to build a simple graph IR with `n` nodes and given edges.
    fn make_ir(n: usize, edges: &[(usize, usize)]) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);

        for i in 0..n {
            let label_id = IrLabelId(ir.labels.len());
            ir.labels.push(IrLabel {
                text: format!("N{i}"),
                span: Span::default(),
            });
            ir.nodes.push(IrNode {
                id: format!("node_{i}"),
                label: Some(label_id),
                ..IrNode::default()
            });
        }

        for &(from, to) in edges {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        ir
    }

    #[test]
    fn laplacian_construction_two_nodes() {
        let ir = make_ir(2, &[(0, 1)]);
        let laplacian = build_laplacian(&ir);

        // L = [[1, -1], [-1, 1]] for a single edge
        assert_eq!(laplacian[(0, 0)], 1.0);
        assert_eq!(laplacian[(1, 1)], 1.0);
        assert_eq!(laplacian[(0, 1)], -1.0);
        assert_eq!(laplacian[(1, 0)], -1.0);
    }

    #[test]
    fn laplacian_row_sums_zero() {
        // For any valid Laplacian, each row must sum to zero
        let ir = make_ir(5, &[(0, 1), (1, 2), (2, 3), (3, 4), (0, 4), (1, 3)]);
        let laplacian = build_laplacian(&ir);

        for i in 0..5 {
            let row_sum: f64 = (0..5).map(|j| laplacian[(i, j)]).sum();
            assert!(row_sum.abs() < 1e-12, "Row {i} sum = {row_sum}, expected 0");
        }
    }

    #[test]
    fn laplacian_symmetry() {
        let ir = make_ir(4, &[(0, 1), (1, 2), (2, 3), (0, 3)]);
        let laplacian = build_laplacian(&ir);

        for i in 0..4 {
            for j in 0..4 {
                assert_eq!(
                    laplacian[(i, j)],
                    laplacian[(j, i)],
                    "Laplacian not symmetric at ({i},{j})"
                );
            }
        }
    }

    #[test]
    fn fiedler_vector_path_graph() {
        // Path graph: 0-1-2-3-4
        // The Fiedler vector should monotonically increase or decrease.
        let ir = make_ir(5, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
        let laplacian = build_laplacian(&ir);
        let result = compute_fiedler_vector(&laplacian);

        assert!(
            result.is_some(),
            "Should compute Fiedler vector for path graph"
        );
        let (fiedler_value, fiedler_vector) = result.unwrap();
        assert!(
            fiedler_value > 0.0,
            "Path graph should be connected (λ₂ > 0)"
        );

        // Check monotonicity (Fiedler vector of path graph is sinusoidal/monotonic)
        let vals: Vec<f64> = (0..5).map(|i| fiedler_vector[i]).collect();
        let increasing = vals.windows(2).all(|w| w[0] <= w[1]);
        let decreasing = vals.windows(2).all(|w| w[0] >= w[1]);
        assert!(
            increasing || decreasing,
            "Fiedler vector of path graph should be monotonic, got {vals:?}"
        );
    }

    #[test]
    fn fiedler_disconnected_graph_separates_components() {
        // Two disconnected components: {0,1} and {2,3}
        let ir = make_ir(4, &[(0, 1), (2, 3)]);
        let laplacian = build_laplacian(&ir);
        let result = compute_fiedler_vector(&laplacian);

        assert!(
            result.is_some(),
            "Disconnected graph should return a valid piece-wise constant vector"
        );
        let (_, fiedler_vector) = result.unwrap();

        let (part0, part1) = spectral_bisect(&[0, 1, 2, 3], &fiedler_vector, 0.2);

        let p0: BTreeSet<usize> = part0.into_iter().collect();
        let p1: BTreeSet<usize> = part1.into_iter().collect();

        let c1: BTreeSet<usize> = [0, 1].into_iter().collect();
        let c2: BTreeSet<usize> = [2, 3].into_iter().collect();

        assert!(
            (p0 == c1 && p1 == c2) || (p0 == c2 && p1 == c1),
            "Should cleanly separate the disconnected components: p0={p0:?}, p1={p1:?}"
        );
    }

    #[test]
    fn bisect_balanced_barbell() {
        // Barbell graph: two cliques of 4 connected by a single bridge
        let mut edges = Vec::new();
        // Clique 0: nodes 0-3
        for i in 0..4 {
            for j in (i + 1)..4 {
                edges.push((i, j));
            }
        }
        // Clique 1: nodes 4-7
        for i in 4..8 {
            for j in (i + 1)..8 {
                edges.push((i, j));
            }
        }
        // Bridge
        edges.push((3, 4));

        let ir = make_ir(8, &edges);
        let result = spectral_bisect_graph(&ir);

        assert!(
            !result.trivial,
            "Barbell graph should produce non-trivial bisection"
        );
        assert_eq!(result.partitions.len(), 2);

        // The optimal cut should separate the two cliques
        let p0: BTreeSet<usize> = result.partitions[0].node_indices.iter().copied().collect();
        let p1: BTreeSet<usize> = result.partitions[1].node_indices.iter().copied().collect();

        // Each partition should have 4 nodes
        assert_eq!(p0.len(), 4);
        assert_eq!(p1.len(), 4);

        // One partition should contain {0,1,2,3} and the other {4,5,6,7}
        let clique0: BTreeSet<usize> = (0..4).collect();
        let clique1: BTreeSet<usize> = (4..8).collect();
        let correct = (p0 == clique0 && p1 == clique1) || (p0 == clique1 && p1 == clique0);
        assert!(
            correct,
            "Should cleanly separate barbell cliques: p0={p0:?}, p1={p1:?}"
        );

        // Only 1 cut edge (the bridge)
        assert_eq!(result.cut_edges, 1, "Bridge should be only cut edge");
    }

    #[test]
    fn bisect_single_node() {
        let ir = make_ir(1, &[]);
        let result = spectral_bisect_graph(&ir);

        assert!(result.trivial);
        assert_eq!(result.partitions.len(), 1);
        assert_eq!(result.partitions[0].node_indices, vec![0]);
    }

    #[test]
    fn bisect_empty_graph() {
        let ir = make_ir(0, &[]);
        let result = spectral_bisect_graph(&ir);

        assert!(result.trivial);
        assert_eq!(result.partitions.len(), 1);
        assert!(result.partitions[0].node_indices.is_empty());
    }

    #[test]
    fn kway_partition_two_cliques() {
        // Two well-separated cliques
        let mut edges = Vec::new();
        for i in 0..5 {
            for j in (i + 1)..5 {
                edges.push((i, j));
            }
        }
        for i in 5..10 {
            for j in (i + 1)..10 {
                edges.push((i, j));
            }
        }
        edges.push((4, 5)); // single bridge

        let ir = make_ir(10, &edges);
        let config = SpectralConfig {
            min_nodes: 2,
            target_partitions: 2,
            balance_threshold: 0.2,
            max_depth: 4,
        };
        let result = spectral_partition_kway(&ir, &config);

        assert!(!result.trivial);
        assert_eq!(result.partitions.len(), 2);

        // Each partition should have 5 nodes
        for p in &result.partitions {
            assert_eq!(p.node_indices.len(), 5);
        }
    }

    #[test]
    fn kway_partition_four_clusters() {
        // Four loosely connected clusters
        let mut edges = Vec::new();
        let cluster_size = 5;
        // 4 clusters of 5 nodes each
        for c in 0..4 {
            let base = c * cluster_size;
            for i in 0..cluster_size {
                for j in (i + 1)..cluster_size {
                    edges.push((base + i, base + j));
                }
            }
        }
        // Connect clusters with single bridges
        edges.push((4, 5));
        edges.push((9, 10));
        edges.push((14, 15));

        let ir = make_ir(20, &edges);
        let config = SpectralConfig {
            min_nodes: 2,
            target_partitions: 4,
            balance_threshold: 0.1,
            max_depth: 4,
        };
        let result = spectral_partition_kway(&ir, &config);

        assert!(!result.trivial);
        // Should produce 4 partitions
        assert_eq!(
            result.partitions.len(),
            4,
            "Should produce 4 partitions for 4-cluster graph, got {}",
            result.partitions.len()
        );

        // Each partition should have 5 nodes
        for (i, p) in result.partitions.iter().enumerate() {
            assert_eq!(
                p.node_indices.len(),
                5,
                "Partition {i} has {} nodes, expected 5",
                p.node_indices.len()
            );
        }
    }

    #[test]
    fn partition_balance_respected() {
        // Line graph: 0-1-2-3-4-5-6-7-8-9
        let edges: Vec<(usize, usize)> = (0..9).map(|i| (i, i + 1)).collect();
        let ir = make_ir(10, &edges);
        let config = SpectralConfig {
            min_nodes: 2,
            target_partitions: 2,
            balance_threshold: 0.3, // Each part should have at least 30% of nodes
            max_depth: 4,
        };
        let result = spectral_bisect_graph_with_config(&ir, &config);

        assert!(!result.trivial);
        for (i, p) in result.partitions.iter().enumerate() {
            let ratio = p.node_indices.len() as f64 / 10.0;
            assert!(
                ratio >= 0.3,
                "Partition {i} has ratio {ratio}, below balance threshold 0.3"
            );
        }
    }

    #[test]
    fn partition_mapping_roundtrip() {
        let ir = make_ir(6, &[(0, 1), (1, 2), (3, 4), (4, 5)]);
        let config = SpectralConfig {
            min_nodes: 2,
            target_partitions: 2,
            balance_threshold: 0.2,
            max_depth: 4,
        };
        let result = spectral_partition_kway(&ir, &config);
        let mapping = build_partition_mapping(&result, 6);

        // Every node should be in exactly one partition
        let mut seen = BTreeSet::new();
        for partition_nodes in &mapping.partitions {
            for &node in partition_nodes {
                assert!(
                    seen.insert(node),
                    "Node {node} appears in multiple partitions"
                );
            }
        }
        assert_eq!(seen.len(), 6, "All nodes should be covered");
    }

    #[test]
    fn should_partition_threshold() {
        let ir = make_ir(10, &[(0, 1)]);
        let config = SpectralConfig {
            min_nodes: 200,
            ..Default::default()
        };
        assert!(!should_partition(&ir, &config));

        let config_small = SpectralConfig {
            min_nodes: 5,
            ..Default::default()
        };
        assert!(should_partition(&ir, &config_small));
    }

    #[test]
    fn normalized_cut_quality() {
        // Barbell graph should have very low normalized cut
        let mut edges = Vec::new();
        for i in 0..5 {
            for j in (i + 1)..5 {
                edges.push((i, j));
            }
        }
        for i in 5..10 {
            for j in (i + 1)..10 {
                edges.push((i, j));
            }
        }
        edges.push((4, 5));

        let ir = make_ir(10, &edges);
        let result = spectral_bisect_graph(&ir);

        assert!(
            result.normalized_cut < 0.5,
            "Barbell graph should have low normalized cut, got {}",
            result.normalized_cut
        );
    }

    #[test]
    fn complete_graph_bisection_balanced() {
        // K6 complete graph — bisection should be balanced
        let mut edges = Vec::new();
        for i in 0..6 {
            for j in (i + 1)..6 {
                edges.push((i, j));
            }
        }
        let ir = make_ir(6, &edges);
        let result = spectral_bisect_graph(&ir);

        assert!(!result.trivial);
        assert_eq!(result.partitions.len(), 2);
        assert_eq!(result.partitions[0].node_indices.len(), 3);
        assert_eq!(result.partitions[1].node_indices.len(), 3);
    }

    #[test]
    fn star_graph_bisection() {
        // Star graph: node 0 connected to all others
        let edges: Vec<(usize, usize)> = (1..8).map(|i| (0, i)).collect();
        let ir = make_ir(8, &edges);
        let result = spectral_bisect_graph(&ir);

        assert!(!result.trivial);
        assert_eq!(result.partitions.len(), 2);

        // Node 0 (hub) should be in one partition
        let total_nodes: usize = result.partitions.iter().map(|p| p.node_indices.len()).sum();
        assert_eq!(total_nodes, 8);
    }
}
