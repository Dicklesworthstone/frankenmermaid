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

/// Result of bounded rewrite exploration for a single layer ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerOptimizationResult {
    /// Best ordering discovered during rewrite exploration.
    pub ordering: LayerOrdering,
    /// Combined crossing count against the fixed adjacent layer(s).
    pub crossing_count: usize,
    /// Number of strictly improving rewrite rounds applied.
    pub rewrites_applied: usize,
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
    if let Some(crossings) = crossing_count_dense(upper, lower, edges) {
        return crossings;
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

fn crossing_count_dense(
    upper: &LayerOrdering,
    lower: &LayerOrdering,
    edges: &LayerEdges,
) -> Option<usize> {
    let max_node_id = max_ordering_node_id(upper, lower, edges)?;
    let domain_len = max_node_id.checked_add(1)?;
    let useful_slots = upper
        .len()
        .saturating_add(lower.len())
        .saturating_add(edges.edges.len().saturating_mul(2))
        .max(64);
    if domain_len > useful_slots.saturating_mul(8) {
        return None;
    }

    let missing = usize::MAX;
    let mut upper_pos = vec![missing; domain_len];
    for (position, &node_id) in upper.order.iter().enumerate() {
        upper_pos[node_id] = position;
    }
    let mut lower_pos = vec![missing; domain_len];
    for (position, &node_id) in lower.order.iter().enumerate() {
        lower_pos[node_id] = position;
    }

    let mut bucket_counts = vec![0_usize; upper.len()];
    let mut valid_edges = 0_usize;
    for &(source, target) in &edges.edges {
        let source_position = upper_pos.get(source).copied().unwrap_or(missing);
        let target_position = lower_pos.get(target).copied().unwrap_or(missing);
        if source_position == missing || target_position == missing {
            continue;
        }
        bucket_counts[source_position] += 1;
        valid_edges += 1;
    }
    if valid_edges < 2 {
        return Some(0);
    }

    let mut offsets = vec![0_usize; upper.len() + 1];
    for (index, count) in bucket_counts.iter().enumerate() {
        offsets[index + 1] = offsets[index] + count;
    }
    let mut cursor = offsets[..upper.len()].to_vec();
    let mut lower_by_upper = vec![0_usize; valid_edges];
    for &(source, target) in &edges.edges {
        let source_position = upper_pos.get(source).copied().unwrap_or(missing);
        let target_position = lower_pos.get(target).copied().unwrap_or(missing);
        if source_position == missing || target_position == missing {
            continue;
        }
        let slot = cursor[source_position];
        lower_by_upper[slot] = target_position;
        cursor[source_position] += 1;
    }

    let mut fenwick = vec![0_usize; lower.len() + 1];
    let mut seen = 0_usize;
    let mut inversions = 0_usize;
    for upper_position in 0..upper.len() {
        let start = offsets[upper_position];
        let end = offsets[upper_position + 1];
        let lowers = &lower_by_upper[start..end];
        for &target_position in lowers {
            inversions += seen - fenwick_sum(&fenwick, target_position + 1);
        }
        for &target_position in lowers {
            fenwick_add(&mut fenwick, target_position + 1);
        }
        seen += lowers.len();
    }

    Some(inversions)
}

fn max_ordering_node_id(
    upper: &LayerOrdering,
    lower: &LayerOrdering,
    edges: &LayerEdges,
) -> Option<usize> {
    upper
        .order
        .iter()
        .chain(&lower.order)
        .chain(
            edges
                .edges
                .iter()
                .flat_map(|(source, target)| [source, target]),
        )
        .copied()
        .max()
}

/// Crossing counter for one adjacent layer pair in which exactly one of the two layers moves.
///
/// `optimize_layer_ordering` scores O(n) candidate orderings per round for O(n) rounds against the
/// *same* neighbour layers, and every one of those scores used to enter [`crossing_count_dense`]
/// and rebuild seven vectors from scratch: two position maps over the node-id domain, bucket
/// counts, offsets, a cursor, the grouped partner positions and the Fenwick tree. On the ER
/// schema-catalog job that showed up as `crossing_count` at 16.3% self-time with `_mi_memset` at
/// 7.1% and mimalloc alloc/free near 9% underneath it.
///
/// The neighbour layer does not move during that search. Its position map, and therefore the
/// bucketing of edges by its positions, are loop invariants. Computed once here, each candidate
/// score becomes a Fenwick sweep over a prebuilt CSR with no allocation at all.
///
/// Counts are identical to [`crossing_count`], not merely close. Two edges cross iff
/// `(u1 - u2)(l1 - l2) < 0`, which is symmetric in the two layers, so bucketing by the *fixed*
/// layer counts the same pairs as bucketing by the upper layer: with partners emitted in ascending
/// fixed position and every group querying the Fenwick tree before adding to it, edges sharing a
/// fixed position are excluded either way.
/// Takes plain slices rather than [`LayerOrdering`]/[`LayerEdges`] so `crossing_refinement`'s
/// transpose and sifting loops -- which hold their orderings as `BTreeMap<usize, Vec<usize>>` and
/// pay the same per-trial rebuild -- can share it without wrapping or cloning.
pub(crate) struct FixedLayerCrossings {
    /// Moving-layer node ids grouped by the fixed layer's position, ascending.
    grouped: Vec<usize>,
    /// `grouped[offsets[p]..offsets[p + 1]]` are the partners of fixed position `p`.
    offsets: Vec<usize>,
    /// Position map over the moving layer's node-id domain. Every candidate is a permutation of
    /// the same node set, so this is fully overwritten per score and never needs clearing.
    positions: Vec<usize>,
    /// Fenwick tree over moving-layer positions; all zero between scores.
    fenwick: Vec<usize>,
}

impl FixedLayerCrossings {
    /// Bucket `edges` by the fixed layer's positions, or `None` when the moving layer's node-id
    /// domain is too sparse to index densely — the same guard [`crossing_count_dense`] applies,
    /// so a pathological id domain still falls back to the allocating path instead of reserving a
    /// huge mostly-empty array.
    pub(crate) fn new(
        fixed: &[usize],
        moving: &[usize],
        edges: &[(usize, usize)],
        fixed_is_upper: bool,
    ) -> Option<Self> {
        const MISSING: usize = usize::MAX;

        let useful_slots = fixed
            .len()
            .saturating_add(moving.len())
            .saturating_add(edges.len().saturating_mul(2))
            .max(64);
        let domain_cap = useful_slots.saturating_mul(8);

        let fixed_domain = fixed.iter().copied().max()?.checked_add(1)?;
        let moving_domain = moving.iter().copied().max()?.checked_add(1)?;
        if fixed_domain > domain_cap || moving_domain > domain_cap {
            return None;
        }

        let mut fixed_pos = vec![MISSING; fixed_domain];
        for (position, &node_id) in fixed.iter().enumerate() {
            fixed_pos[node_id] = position;
        }
        let mut positions = vec![MISSING; moving_domain];
        for (position, &node_id) in moving.iter().enumerate() {
            positions[node_id] = position;
        }

        // Endpoint absent from its own layer means the edge can never cross anything here, and
        // membership does not change across candidates, so filter once at build time.
        let split = |&(source, target): &(usize, usize)| -> Option<(usize, usize)> {
            let (fixed_node, moving_node) = if fixed_is_upper {
                (source, target)
            } else {
                (target, source)
            };
            let fixed_position = fixed_pos.get(fixed_node).copied().unwrap_or(MISSING);
            if fixed_position == MISSING
                || positions.get(moving_node).copied().unwrap_or(MISSING) == MISSING
            {
                return None;
            }
            Some((fixed_position, moving_node))
        };

        let mut offsets = vec![0_usize; fixed.len().saturating_add(1)];
        for (fixed_position, _) in edges.iter().filter_map(split) {
            offsets[fixed_position + 1] += 1;
        }
        for index in 0..fixed.len() {
            offsets[index + 1] += offsets[index];
        }

        let valid_edges = offsets[fixed.len()];
        let mut grouped = vec![0_usize; valid_edges];
        let mut cursor = offsets[..fixed.len()].to_vec();
        for (fixed_position, moving_node) in edges.iter().filter_map(split) {
            grouped[cursor[fixed_position]] = moving_node;
            cursor[fixed_position] += 1;
        }

        Some(Self {
            grouped,
            offsets,
            positions,
            fenwick: vec![0_usize; moving.len().saturating_add(1)],
        })
    }

    /// Crossings against the fixed layer for one candidate ordering of the moving layer.
    pub(crate) fn count(&mut self, moving: &[usize]) -> usize {
        if self.grouped.len() < 2 {
            return 0;
        }
        for (position, &node_id) in moving.iter().enumerate() {
            self.positions[node_id] = position;
        }

        let mut seen = 0_usize;
        let mut inversions = 0_usize;
        for window in self.offsets.windows(2) {
            let (Some(&start), Some(&end)) = (window.first(), window.get(1)) else {
                continue;
            };
            let group = &self.grouped[start..end];
            for &node_id in group {
                let position = self.positions[node_id];
                inversions += seen - fenwick_sum(&self.fenwick, position + 1);
            }
            for &node_id in group {
                fenwick_add(&mut self.fenwick, self.positions[node_id] + 1);
            }
            seen += group.len();
        }

        self.fenwick.fill(0);
        inversions
    }
}

/// Scores candidate orderings of one layer against its fixed neighbours.
///
/// Holds a [`FixedLayerCrossings`] per side where the node-id domain allows it and falls back to
/// [`crossing_count`] otherwise; both produce the same counts.
struct LayerScorer<'a> {
    upper: Option<(&'a LayerOrdering, &'a LayerEdges)>,
    lower: Option<(&'a LayerOrdering, &'a LayerEdges)>,
    upper_fast: Option<FixedLayerCrossings>,
    lower_fast: Option<FixedLayerCrossings>,
}

impl<'a> LayerScorer<'a> {
    fn new(
        current: &LayerOrdering,
        upper: Option<(&'a LayerOrdering, &'a LayerEdges)>,
        lower: Option<(&'a LayerOrdering, &'a LayerEdges)>,
    ) -> Self {
        Self {
            upper,
            lower,
            upper_fast: upper.and_then(|(ordering, edges)| {
                FixedLayerCrossings::new(&ordering.order, &current.order, &edges.edges, true)
            }),
            lower_fast: lower.and_then(|(ordering, edges)| {
                FixedLayerCrossings::new(&ordering.order, &current.order, &edges.edges, false)
            }),
        }
    }

    fn score(&mut self, current: &LayerOrdering) -> usize {
        let mut total = 0_usize;
        match (self.upper_fast.as_mut(), self.upper) {
            (Some(fast), _) => total += fast.count(&current.order),
            (None, Some((ordering, edges))) => total += crossing_count(ordering, current, edges),
            (None, None) => {}
        }
        match (self.lower_fast.as_mut(), self.lower) {
            (Some(fast), _) => total += fast.count(&current.order),
            (None, Some((ordering, edges))) => total += crossing_count(current, ordering, edges),
            (None, None) => {}
        }
        total
    }
}

fn fenwick_sum(tree: &[usize], mut index: usize) -> usize {
    let mut sum = 0_usize;
    while index > 0 {
        sum += tree[index];
        index -= index & index.wrapping_neg();
    }
    sum
}

fn fenwick_add(tree: &mut [usize], mut index: usize) {
    while index < tree.len() {
        tree[index] += 1;
        index += index & index.wrapping_neg();
    }
}

/// Compute the combined crossing count contributed by one layer against its adjacent layers.
///
/// When both adjacent layers are present, this sums crossings for:
/// - `upper -> current`
/// - `current -> lower`
#[must_use]
pub fn local_crossing_count(
    current: &LayerOrdering,
    upper: Option<(&LayerOrdering, &LayerEdges)>,
    lower: Option<(&LayerOrdering, &LayerEdges)>,
) -> usize {
    let mut total = 0;
    if let Some((upper_ordering, upper_edges)) = upper {
        total += crossing_count(upper_ordering, current, upper_edges);
    }
    if let Some((lower_ordering, lower_edges)) = lower {
        total += crossing_count(current, lower_ordering, lower_edges);
    }
    total
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

/// Estimated e-graph memory above which it is worth proving that a saturated rank has no
/// ordering leverage before constructing rewrite candidates.
const NO_LEVERAGE_GATE_MIN_BYTES: usize = 64 * 1024;

/// Decide whether bounded rewrite exploration can change the crossing count for one rank.
///
/// The width limit remains the primary liveness bound.  Above a material estimated allocation,
/// this additionally rejects a rank only when every incident adjacent-rank pair is a complete
/// bipartite graph.  In that case every permutation produces the same edge-position sequence, so
/// the local crossing count is invariant and e-graph exploration cannot improve the layout.
#[must_use]
pub fn should_optimize_egraph_layer(
    current: &LayerOrdering,
    upper: Option<(&LayerOrdering, &LayerEdges)>,
    lower: Option<(&LayerOrdering, &LayerEdges)>,
) -> bool {
    if current.len() < 2 || !should_use_egraph(current.len()) {
        return false;
    }

    let (_, estimated_bytes) = estimate_egraph_size(current.len());
    if estimated_bytes < NO_LEVERAGE_GATE_MIN_BYTES {
        return true;
    }

    let no_upper_leverage =
        upper.is_none_or(|(ordering, edges)| is_complete_bipartite(ordering, current, edges));
    let no_lower_leverage =
        lower.is_none_or(|(ordering, edges)| is_complete_bipartite(current, ordering, edges));

    !(no_upper_leverage && no_lower_leverage)
}

fn is_complete_bipartite(upper: &LayerOrdering, lower: &LayerOrdering, edges: &LayerEdges) -> bool {
    let Some(expected_edges) = upper.len().checked_mul(lower.len()) else {
        return false;
    };
    if edges.edges.len() != expected_edges {
        return false;
    }

    let upper_nodes: BTreeMap<usize, ()> =
        upper.order.iter().copied().map(|node| (node, ())).collect();
    let lower_nodes: BTreeMap<usize, ()> =
        lower.order.iter().copied().map(|node| (node, ())).collect();
    let mut seen = BTreeMap::new();
    edges.edges.iter().all(|&(source, target)| {
        upper_nodes.contains_key(&source)
            && lower_nodes.contains_key(&target)
            && seen.insert((source, target), ()).is_none()
    })
}

fn median_insert_candidates(
    ordering: &LayerOrdering,
    upper: Option<(&LayerOrdering, &LayerEdges)>,
    lower: Option<(&LayerOrdering, &LayerEdges)>,
) -> Vec<LayerOrdering> {
    let mut candidates = Vec::new();

    for &node_id in &ordering.order {
        if let Some((upper_ordering, upper_edges)) = upper
            && let Some(target_pos) = median_position(node_id, upper_ordering, upper_edges, false)
            && let Some(candidate) = move_node(ordering, node_id, target_pos)
        {
            candidates.push(candidate);
        }

        if let Some((lower_ordering, lower_edges)) = lower
            && let Some(target_pos) = median_position(node_id, lower_ordering, lower_edges, true)
            && let Some(candidate) = move_node(ordering, node_id, target_pos)
        {
            candidates.push(candidate);
        }
    }

    candidates
}

fn block_rotation_candidates(ordering: &LayerOrdering) -> Vec<LayerOrdering> {
    let mut candidates = Vec::new();
    let n = ordering.len();
    for len in 3..=n.min(4) {
        for start in 0..=n - len {
            for amount in 1..len {
                if let Some(candidate) = block_rotate(ordering, start, len, amount) {
                    candidates.push(candidate);
                }
            }
        }
    }
    candidates
}

fn candidate_orderings(
    ordering: &LayerOrdering,
    upper: Option<(&LayerOrdering, &LayerEdges)>,
    lower: Option<(&LayerOrdering, &LayerEdges)>,
) -> Vec<LayerOrdering> {
    let mut candidates = all_adjacent_swaps(ordering);
    candidates.extend(median_insert_candidates(ordering, upper, lower));
    candidates.extend(block_rotation_candidates(ordering));
    candidates.sort_by(|left, right| left.order.cmp(&right.order));
    candidates.dedup_by(|left, right| left.order == right.order);
    candidates
}

/// Run a bounded deterministic rewrite search over a layer ordering.
///
/// This is the practical integration wedge for the e-graph work: we enumerate
/// rewrite neighbors (adjacent swaps, median insertions, small block rotations),
/// score them against the fixed adjacent layer(s), and repeatedly take the best
/// strictly improving rewrite until a local optimum is reached.
#[must_use]
pub fn optimize_layer_ordering(
    ordering: &LayerOrdering,
    upper: Option<(&LayerOrdering, &LayerEdges)>,
    lower: Option<(&LayerOrdering, &LayerEdges)>,
) -> LayerOptimizationResult {
    let mut best = ordering.clone();
    if best.len() < 2 || !should_use_egraph(best.len()) {
        return LayerOptimizationResult {
            crossing_count: local_crossing_count(&best, upper, lower),
            ordering: best,
            rewrites_applied: 0,
        };
    }

    // The neighbour layers are fixed for this whole search, so bucket the edges against them once
    // instead of once per candidate. See `FixedLayerCrossings`.
    let mut scorer = LayerScorer::new(&best, upper, lower);
    let mut best_crossings = scorer.score(&best);

    let mut rewrites_applied = 0;
    let max_rounds = best.len().saturating_mul(2).max(1);
    for _ in 0..max_rounds {
        let mut best_candidate: Option<(usize, LayerOrdering)> = None;
        for candidate in candidate_orderings(&best, upper, lower) {
            let candidate_crossings = scorer.score(&candidate);
            if candidate_crossings >= best_crossings {
                continue;
            }

            match &mut best_candidate {
                Some((current_best_crossings, current_best_ordering)) => {
                    if candidate_crossings < *current_best_crossings
                        || (candidate_crossings == *current_best_crossings
                            && candidate.order < current_best_ordering.order)
                    {
                        *current_best_crossings = candidate_crossings;
                        *current_best_ordering = candidate;
                    }
                }
                None => {
                    best_candidate = Some((candidate_crossings, candidate));
                }
            }
        }

        let Some((candidate_crossings, candidate)) = best_candidate else {
            break;
        };

        best = candidate;
        best_crossings = candidate_crossings;
        rewrites_applied += 1;
        if best_crossings == 0 {
            break;
        }
    }

    LayerOptimizationResult {
        ordering: best,
        crossing_count: best_crossings,
        rewrites_applied,
    }
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
    fn crossing_count_edges_from_same_source_do_not_self_cross() {
        let upper = LayerOrdering::new(vec![0, 1]);
        let lower = LayerOrdering::new(vec![2, 3]);
        let edges = LayerEdges {
            edges: vec![(0, 3), (0, 2), (1, 3)],
        };

        assert_eq!(crossing_count(&upper, &lower, &edges), 0);
    }

    #[test]
    fn crossing_count_handles_sparse_node_ids() {
        let upper = LayerOrdering::new(vec![10, 1_000_000]);
        let lower = LayerOrdering::new(vec![20, 2_000_000]);
        let edges = LayerEdges {
            edges: vec![(10, 2_000_000), (1_000_000, 20)],
        };

        assert_eq!(crossing_count(&upper, &lower, &edges), 1);
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

    /// The hoisted counter must agree with `crossing_count` exactly, from either side, including
    /// when an edge names a node that is absent from the layer it is supposed to sit in.
    #[test]
    fn fixed_layer_crossings_match_crossing_count() {
        let mut state = 0x2b1f_c0de_u64;
        let mut next = |bound: usize| -> usize {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(state >> 33).unwrap_or(0) % bound.max(1)
        };

        for case in 0..400 {
            let upper_len = 1 + next(9);
            let lower_len = 1 + next(9);
            let upper = LayerOrdering::new((0..upper_len).map(|i| i * 2).collect());
            let lower = LayerOrdering::new((0..lower_len).map(|i| 1 + i * 2).collect());

            let mut edges = Vec::new();
            for _ in 0..next(14) {
                // A tenth of the endpoints deliberately name nodes outside both layers.
                let source = if next(10) == 0 {
                    900 + next(5)
                } else {
                    next(upper_len) * 2
                };
                let target = if next(10) == 0 {
                    900 + next(5)
                } else {
                    1 + next(lower_len) * 2
                };
                edges.push((source, target));
            }
            let edges = LayerEdges { edges };

            let expected = crossing_count(&upper, &lower, &edges);
            let from_upper =
                FixedLayerCrossings::new(&upper.order, &lower.order, &edges.edges, true)
                    .map(|mut counter| counter.count(&lower.order));
            let from_lower =
                FixedLayerCrossings::new(&lower.order, &upper.order, &edges.edges, false)
                    .map(|mut counter| counter.count(&upper.order));
            assert_eq!(from_upper, Some(expected), "case {case} bucketed by upper");
            assert_eq!(from_lower, Some(expected), "case {case} bucketed by lower");

            // Reusing one counter across permutations must match a fresh count every time.
            let Some(mut counter) =
                FixedLayerCrossings::new(&upper.order, &lower.order, &edges.edges, true)
            else {
                continue;
            };
            let mut permuted = lower.order.clone();
            for _ in 0..6 {
                let i = next(lower_len);
                let j = next(lower_len);
                permuted.swap(i, j);
                let candidate = LayerOrdering::new(permuted.clone());
                assert_eq!(
                    counter.count(&candidate.order),
                    crossing_count(&upper, &candidate, &edges),
                    "case {case} reused counter"
                );
            }
        }
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
    fn local_crossing_count_sums_both_adjacent_pairs() {
        let upper = LayerOrdering::new(vec![0, 1, 2]);
        let current = LayerOrdering::new(vec![4, 3, 5]);
        let lower = LayerOrdering::new(vec![6, 7, 8]);
        let upper_edges = LayerEdges {
            edges: vec![(0, 3), (1, 4)],
        };
        let lower_edges = LayerEdges {
            edges: vec![(4, 6), (4, 7)],
        };

        assert_eq!(
            local_crossing_count(
                &current,
                Some((&upper, &upper_edges)),
                Some((&lower, &lower_edges))
            ),
            1
        );
    }

    #[test]
    fn optimize_layer_ordering_improves_combined_crossings() {
        let upper = LayerOrdering::new(vec![0, 1, 2]);
        let current = LayerOrdering::new(vec![4, 3, 5]);
        let lower = LayerOrdering::new(vec![6, 7, 8]);
        let upper_edges = LayerEdges {
            edges: vec![(0, 3), (1, 4)],
        };
        let lower_edges = LayerEdges {
            edges: vec![(4, 6), (4, 7)],
        };

        let result = optimize_layer_ordering(
            &current,
            Some((&upper, &upper_edges)),
            Some((&lower, &lower_edges)),
        );

        assert_eq!(result.ordering.order, vec![3, 4, 5]);
        assert_eq!(result.crossing_count, 0);
        assert!(result.rewrites_applied > 0);
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
