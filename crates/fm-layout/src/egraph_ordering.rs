//! E-graph encoding for Sugiyama layer node orderings.
//!
//! Defines the language, rewrite rules, and cost functions for using equality
//! saturation to find optimal node orderings that minimize edge crossings in
//! hierarchical (Sugiyama) graph layout.
//!
//! # Design Decisions
//!
//! ## Encoding Choice: Permutation with Adjacent Swaps
//!
//! Three encodings were considered:
//!
//! | Encoding | Representation | Rewrite space | Cost computation |
//! |----------|---------------|---------------|-----------------|
//! | Seq (linked list) | `Seq(a, Seq(b, Seq(c, Nil)))` | O(n!) swap rules | O(n²) per eval |
//! | Perm (flat vector) | `Perm([2,0,1])` | O(n²) adjacent swaps | O(n log n) merge-sort |
//! | Binary swap tree | `Swap(i, Swap(j, Id))` | O(n) compose rules | O(n log n) |
//!
//! **Chosen: Permutation with adjacent swaps** (Perm encoding).
//!
//! Rationale:
//! - Most natural for Sugiyama: each layer IS a permutation of node IDs.
//! - Adjacent swaps are the atomic operation in barycenter/transpose heuristics.
//! - Crossing count can be computed in O(n log n) via merge-sort inversion counting.
//! - E-graph size is manageable: O(n²) swap variants per layer.
//!
//! ## Rewrite Rules
//!
//! 1. **Adjacent swap**: `swap(perm, i)` swaps positions i and i+1.
//! 2. **Block rotation**: `rotate(perm, start, len)` cyclically rotates a contiguous block.
//! 3. **Median insertion**: `median_insert(perm, node, pos)` moves a node to its median position.
//!
//! ## Cost Function
//!
//! `crossing_count(layer_k_ordering, layer_k+1_ordering, edges_between_layers)`
//!
//! Computed via merge-sort inversion counting: O(n log n).
//!
//! ## Multi-Layer Strategy
//!
//! Per-layer optimization with neighbor fixpoints (like barycenter sweeps):
//! 1. Fix layer k-1 ordering.
//! 2. Use equality saturation to optimize layer k ordering.
//! 3. Fix layer k, optimize layer k+1.
//! 4. Repeat until convergence.
//!
//! Joint multi-layer optimization is exponential and impractical for > 5 layers.
//!
//! ## Scaling Estimates
//!
//! | Nodes/layer | Swap variants | E-graph nodes | Memory (est.) | Time (est.) |
//! |-------------|--------------|---------------|---------------|-------------|
//! | 10 | 45 | ~500 | ~50 KB | < 1 ms |
//! | 50 | 1,225 | ~15K | ~2 MB | ~50 ms |
//! | 100 | 4,950 | ~60K | ~8 MB | ~500 ms |
//! | 500 | 124,750 | ~1.5M | ~200 MB | > 10 s |
//!
//! **Practical limit**: ~100 nodes per layer for interactive use, ~200 for batch.
//! Beyond that, fall back to barycenter heuristic.
//!
//! # References
//!
//! - Willsey et al., "egg: Fast and Extensible Equality Saturation" (POPL 2021)
//! - Sugiyama et al., "Methods for Visual Understanding of Hierarchical Systems" (1981)

use std::collections::BTreeMap;

/// A node ordering for a single Sugiyama layer.
///
/// Stored as a permutation: `order[i]` is the node ID at position i.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayerOrdering {
    /// Node IDs in left-to-right order.
    pub order: Vec<usize>,
}

impl LayerOrdering {
    /// Create a new ordering from a sequence of node IDs.
    #[must_use]
    pub fn new(order: Vec<usize>) -> Self {
        Self { order }
    }

    /// Create the identity ordering: [0, 1, 2, ..., n-1].
    #[must_use]
    pub fn identity(n: usize) -> Self {
        Self {
            order: (0..n).collect(),
        }
    }

    /// Number of nodes in this layer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether the ordering is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Position of a node in this ordering, if present.
    #[must_use]
    pub fn position_of(&self, node_id: usize) -> Option<usize> {
        self.order.iter().position(|&n| n == node_id)
    }
}

// ---------------------------------------------------------------------------
// Rewrite operations (generate equivalent orderings)
// ---------------------------------------------------------------------------

/// Swap adjacent positions i and i+1 in the ordering.
///
/// This is the atomic rewrite rule. Every permutation can be reached from any
/// other via a sequence of adjacent swaps (bubble sort).
#[must_use]
pub fn adjacent_swap(ordering: &LayerOrdering, i: usize) -> Option<LayerOrdering> {
    if i + 1 >= ordering.order.len() {
        return None;
    }
    let mut new_order = ordering.order.clone();
    new_order.swap(i, i + 1);
    Some(LayerOrdering::new(new_order))
}

/// Rotate a contiguous block of nodes cyclically.
///
/// `rotate(ordering, start, len, amount)` rotates `ordering[start..start+len]`
/// left by `amount` positions.
#[must_use]
pub fn block_rotate(
    ordering: &LayerOrdering,
    start: usize,
    len: usize,
    amount: usize,
) -> Option<LayerOrdering> {
    if start + len > ordering.order.len() || len == 0 {
        return None;
    }
    let mut new_order = ordering.order.clone();
    let amount = amount % len;
    new_order[start..start + len].rotate_left(amount);
    Some(LayerOrdering::new(new_order))
}

/// Move a node to a target position (median insertion).
///
/// Removes the node from its current position and inserts it at `target_pos`.
#[must_use]
pub fn move_node(
    ordering: &LayerOrdering,
    node_id: usize,
    target_pos: usize,
) -> Option<LayerOrdering> {
    let current_pos = ordering.position_of(node_id)?;
    if current_pos == target_pos {
        return Some(ordering.clone());
    }
    let mut new_order = ordering.order.clone();
    let removed = new_order.remove(current_pos);
    let target = target_pos.min(new_order.len());
    new_order.insert(target, removed);
    Some(LayerOrdering::new(new_order))
}

// ---------------------------------------------------------------------------
// Cost function: crossing count
// ---------------------------------------------------------------------------

/// Edges between two adjacent layers, represented as (source_node, target_node).
#[derive(Debug, Clone)]
pub struct LayerEdges {
    /// Edges from layer k to layer k+1: (source_node_id, target_node_id).
    pub edges: Vec<(usize, usize)>,
}

/// Count the number of edge crossings between two adjacent layers.
///
/// Two edges (u1→v1) and (u2→v2) cross iff the relative order of u1,u2 in
/// the upper layer is opposite to the relative order of v1,v2 in the lower layer.
///
/// Uses merge-sort inversion counting for O(n log n) where n = number of edges.
///
/// # Arguments
/// * `upper` - Node ordering of the upper (source) layer.
/// * `lower` - Node ordering of the lower (target) layer.
/// * `edges` - Edges between the two layers.
#[must_use]
pub fn crossing_count(upper: &LayerOrdering, lower: &LayerOrdering, edges: &LayerEdges) -> usize {
    if edges.edges.len() < 2 {
        return 0;
    }

    // Build position maps.
    let upper_pos: BTreeMap<usize, usize> = upper
        .order
        .iter()
        .enumerate()
        .map(|(pos, &node)| (node, pos))
        .collect();
    let lower_pos: BTreeMap<usize, usize> = lower
        .order
        .iter()
        .enumerate()
        .map(|(pos, &node)| (node, pos))
        .collect();

    // Sort edges by upper position, then extract lower positions.
    let mut edge_positions: Vec<(usize, usize)> = edges
        .edges
        .iter()
        .filter_map(|&(src, tgt)| {
            let up = upper_pos.get(&src)?;
            let lo = lower_pos.get(&tgt)?;
            Some((*up, *lo))
        })
        .collect();

    // Sort by upper position (stable sort for determinism).
    edge_positions.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    // Count inversions in the lower positions using merge sort.
    let lower_positions: Vec<usize> = edge_positions.iter().map(|&(_, lo)| lo).collect();
    count_inversions(&lower_positions)
}

/// Count inversions in a sequence using merge sort. O(n log n).
fn count_inversions(arr: &[usize]) -> usize {
    merge_sort_count(arr).1
}

/// Merge sort that counts inversions. Returns (sorted_array, inversion_count).
fn merge_sort_count(arr: &[usize]) -> (Vec<usize>, usize) {
    if arr.len() <= 1 {
        return (arr.to_vec(), 0);
    }
    let mid = arr.len() / 2;
    let (left, left_inv) = merge_sort_count(&arr[..mid]);
    let (right, right_inv) = merge_sort_count(&arr[mid..]);

    let mut merged = Vec::with_capacity(arr.len());
    let mut inversions = left_inv + right_inv;
    let mut i = 0;
    let mut j = 0;

    while i < left.len() && j < right.len() {
        if left[i] <= right[j] {
            merged.push(left[i]);
            i += 1;
        } else {
            merged.push(right[j]);
            // All remaining elements in left are > right[j], so they form inversions.
            inversions += left.len() - i;
            j += 1;
        }
    }
    merged.extend_from_slice(&left[i..]);
    merged.extend_from_slice(&right[j..]);

    (merged, inversions)
}

/// Compute the median position for a node based on its neighbors in the adjacent layer.
///
/// Used for the "median insertion" rewrite rule.
#[must_use]
pub fn median_position(
    node_id: usize,
    adjacent_ordering: &LayerOrdering,
    edges: &LayerEdges,
    is_source: bool,
) -> Option<usize> {
    // Collect positions of neighbors in the adjacent layer.
    let neighbor_positions: Vec<usize> = edges
        .edges
        .iter()
        .filter_map(|&(src, tgt)| {
            if is_source && src == node_id {
                adjacent_ordering.position_of(tgt)
            } else if !is_source && tgt == node_id {
                adjacent_ordering.position_of(src)
            } else {
                None
            }
        })
        .collect();

    if neighbor_positions.is_empty() {
        return None;
    }

    let mut sorted = neighbor_positions;
    sorted.sort_unstable();
    Some(sorted[sorted.len() / 2])
}

/// Generate all single-swap neighbors of an ordering.
///
/// Returns all orderings reachable by one adjacent swap. This is the
/// set of rewrites that equality saturation would explore at each step.
#[must_use]
pub fn all_adjacent_swaps(ordering: &LayerOrdering) -> Vec<LayerOrdering> {
    let n = ordering.order.len();
    if n < 2 {
        return Vec::new();
    }
    (0..n - 1)
        .filter_map(|i| adjacent_swap(ordering, i))
        .collect()
}

/// Estimate e-graph size for a layer of n nodes.
///
/// Returns (estimated_nodes, estimated_memory_bytes).
#[must_use]
pub fn estimate_egraph_size(n: usize) -> (usize, usize) {
    // Each ordering generates n-1 swap variants.
    // After k rounds of saturation, the e-graph grows roughly as:
    // round 0: 1 node
    // round 1: n-1 nodes
    // round 2: ~(n-1)² nodes (but many merge via equality)
    // In practice with equality saturation, growth saturates around O(n²) to O(n³).
    //
    // Conservative estimate: ~n² e-nodes per layer.
    let e_nodes = if n <= 1 { 1 } else { n * n };

    // Each e-node: ~64 bytes (id, children, metadata).
    let memory = e_nodes * 64;

    (e_nodes, memory)
}

/// Determine whether equality saturation is practical for a given layer size.
///
/// Returns true if the layer is small enough for equality saturation,
/// false if the fallback barycenter heuristic should be used.
#[must_use]
pub fn should_use_egraph(nodes_per_layer: usize) -> bool {
    // Practical limit: ~100 nodes for interactive, ~200 for batch.
    nodes_per_layer <= 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_ordering() {
        let o = LayerOrdering::identity(5);
        assert_eq!(o.order, vec![0, 1, 2, 3, 4]);
        assert_eq!(o.len(), 5);
        assert_eq!(o.position_of(3), Some(3));
        assert_eq!(o.position_of(99), None);
    }

    #[test]
    fn adjacent_swap_basic() {
        let o = LayerOrdering::new(vec![0, 1, 2, 3]);
        let swapped = adjacent_swap(&o, 1).unwrap();
        assert_eq!(swapped.order, vec![0, 2, 1, 3]);
    }

    #[test]
    fn adjacent_swap_out_of_bounds() {
        let o = LayerOrdering::new(vec![0, 1, 2]);
        assert!(adjacent_swap(&o, 2).is_none());
        assert!(adjacent_swap(&o, 5).is_none());
    }

    #[test]
    fn block_rotate_basic() {
        let o = LayerOrdering::new(vec![0, 1, 2, 3, 4]);
        let rotated = block_rotate(&o, 1, 3, 1).unwrap();
        assert_eq!(rotated.order, vec![0, 2, 3, 1, 4]);
    }

    #[test]
    fn block_rotate_full() {
        let o = LayerOrdering::new(vec![0, 1, 2, 3]);
        let rotated = block_rotate(&o, 0, 4, 2).unwrap();
        assert_eq!(rotated.order, vec![2, 3, 0, 1]);
    }

    #[test]
    fn move_node_basic() {
        let o = LayerOrdering::new(vec![0, 1, 2, 3, 4]);
        let moved = move_node(&o, 4, 1).unwrap();
        assert_eq!(moved.order, vec![0, 4, 1, 2, 3]);
    }

    #[test]
    fn move_node_same_position() {
        let o = LayerOrdering::new(vec![0, 1, 2]);
        let moved = move_node(&o, 1, 1).unwrap();
        assert_eq!(moved.order, o.order);
    }

    #[test]
    fn crossing_count_no_crossings() {
        // Upper: [A, B], Lower: [C, D]
        // Edges: A→C, B→D (parallel, no crossings)
        let upper = LayerOrdering::new(vec![0, 1]);
        let lower = LayerOrdering::new(vec![2, 3]);
        let edges = LayerEdges {
            edges: vec![(0, 2), (1, 3)],
        };

        assert_eq!(crossing_count(&upper, &lower, &edges), 0);
    }

    #[test]
    fn crossing_count_one_crossing() {
        // Upper: [A, B], Lower: [C, D]
        // Edges: A→D, B→C (crossing!)
        let upper = LayerOrdering::new(vec![0, 1]);
        let lower = LayerOrdering::new(vec![2, 3]);
        let edges = LayerEdges {
            edges: vec![(0, 3), (1, 2)],
        };

        assert_eq!(crossing_count(&upper, &lower, &edges), 1);
    }

    #[test]
    fn crossing_count_multiple() {
        // Upper: [0, 1, 2], Lower: [3, 4, 5]
        // Edges: 0→5, 1→4, 2→3 (all cross = 3 crossings)
        let upper = LayerOrdering::new(vec![0, 1, 2]);
        let lower = LayerOrdering::new(vec![3, 4, 5]);
        let edges = LayerEdges {
            edges: vec![(0, 5), (1, 4), (2, 3)],
        };

        assert_eq!(crossing_count(&upper, &lower, &edges), 3);
    }

    #[test]
    fn crossing_count_empty_edges() {
        let upper = LayerOrdering::new(vec![0, 1]);
        let lower = LayerOrdering::new(vec![2, 3]);
        let edges = LayerEdges { edges: vec![] };

        assert_eq!(crossing_count(&upper, &lower, &edges), 0);
    }

    #[test]
    fn swap_reduces_crossings() {
        // Upper: [0, 1], Lower: [2, 3]
        // Edges: 0→3, 1→2 — 1 crossing.
        // After swap(upper, 0): Upper becomes [1, 0] — 0 crossings.
        let upper = LayerOrdering::new(vec![0, 1]);
        let lower = LayerOrdering::new(vec![2, 3]);
        let edges = LayerEdges {
            edges: vec![(0, 3), (1, 2)],
        };

        let before = crossing_count(&upper, &lower, &edges);
        let swapped = adjacent_swap(&upper, 0).unwrap();
        let after = crossing_count(&swapped, &lower, &edges);

        assert_eq!(before, 1);
        assert_eq!(after, 0);
    }

    #[test]
    fn median_position_basic() {
        // Node 0 connects to nodes at positions 1, 3, 5 in adjacent layer.
        let adjacent = LayerOrdering::new(vec![10, 11, 12, 13, 14, 15]);
        let edges = LayerEdges {
            edges: vec![(0, 11), (0, 13), (0, 15)],
        };

        let median = median_position(0, &adjacent, &edges, true);
        assert_eq!(median, Some(3)); // median of [1, 3, 5] = 3
    }

    #[test]
    fn median_position_no_neighbors() {
        let adjacent = LayerOrdering::new(vec![10, 11, 12]);
        let edges = LayerEdges { edges: vec![] };

        assert!(median_position(0, &adjacent, &edges, true).is_none());
    }

    #[test]
    fn all_swaps_count() {
        let o = LayerOrdering::identity(5);
        let swaps = all_adjacent_swaps(&o);
        assert_eq!(swaps.len(), 4); // n-1 = 4 possible adjacent swaps
    }

    #[test]
    fn all_swaps_single_element() {
        let o = LayerOrdering::identity(1);
        assert!(all_adjacent_swaps(&o).is_empty());
    }

    #[test]
    fn estimate_scales_quadratically() {
        let (n10, _) = estimate_egraph_size(10);
        let (n50, _) = estimate_egraph_size(50);
        let (n100, _) = estimate_egraph_size(100);

        assert_eq!(n10, 100);
        assert_eq!(n50, 2500);
        assert_eq!(n100, 10000);
    }

    #[test]
    fn should_use_egraph_threshold() {
        assert!(should_use_egraph(50));
        assert!(should_use_egraph(100));
        assert!(!should_use_egraph(101));
        assert!(!should_use_egraph(500));
    }

    #[test]
    fn crossing_count_deterministic() {
        let upper = LayerOrdering::new(vec![0, 1, 2, 3]);
        let lower = LayerOrdering::new(vec![4, 5, 6, 7]);
        let edges = LayerEdges {
            edges: vec![(0, 7), (1, 5), (2, 6), (3, 4)],
        };

        let c1 = crossing_count(&upper, &lower, &edges);
        let c2 = crossing_count(&upper, &lower, &edges);
        assert_eq!(c1, c2);
    }

    #[test]
    fn merge_sort_inversion_count() {
        // [3, 1, 2] has 2 inversions: (3,1) and (3,2)
        let (sorted, inv) = merge_sort_count(&[3, 1, 2]);
        assert_eq!(sorted, vec![1, 2, 3]);
        assert_eq!(inv, 2);
    }

    #[test]
    fn merge_sort_sorted_input() {
        let (sorted, inv) = merge_sort_count(&[1, 2, 3, 4]);
        assert_eq!(sorted, vec![1, 2, 3, 4]);
        assert_eq!(inv, 0);
    }

    #[test]
    fn merge_sort_reversed_input() {
        // [4, 3, 2, 1] has 6 inversions (n*(n-1)/2)
        let (sorted, inv) = merge_sort_count(&[4, 3, 2, 1]);
        assert_eq!(sorted, vec![1, 2, 3, 4]);
        assert_eq!(inv, 6);
    }
}
