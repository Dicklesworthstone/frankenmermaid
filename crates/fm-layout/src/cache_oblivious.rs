//! Cache-oblivious data layout for large diagram traversal.
//!
//! Organizes node and edge data in memory to minimize cache misses during layout
//! algorithms. Uses Morton codes (Z-order curves) for spatial locality and blocked
//! edge iteration for per-layer traversals.
//!
//! # Techniques
//!
//! 1. **Morton code ordering**: Nodes stored in Z-order (bit-interleaved x,y coordinates)
//!    so that spatially close nodes are also close in memory.
//!
//! 2. **Blocked edge groups**: Edges grouped by source rank for Sugiyama crossing
//!    minimization, ensuring per-layer iterations touch contiguous memory.
//!
//! 3. **Van Emde Boas layout**: Tree nodes stored in recursive halving order so that
//!    subtrees fit in cache lines.
//!
//! # References
//!
//! - Frigo et al., "Cache-Oblivious Algorithms" (FOCS 1999)
//! - Bender et al., "Cache-Oblivious B-Trees" (SICOMP 2005)

/// Compute the Morton code (Z-order curve value) for 2D integer coordinates.
///
/// Interleaves the bits of x and y to produce a single u64 that preserves
/// 2D spatial locality. Points close in 2D space have close Morton codes.
///
/// Input coordinates are clamped to 32 bits each (the interleaved result is 64 bits).
#[must_use]
pub fn morton_code(x: u32, y: u32) -> u64 {
    interleave_bits(u64::from(x)) | (interleave_bits(u64::from(y)) << 1)
}

/// Spread a 32-bit value into even bits of a 64-bit value.
/// E.g., 0b1011 → 0b01_00_01_01 (each bit separated by a zero bit).
fn interleave_bits(mut v: u64) -> u64 {
    v &= 0x0000_0000_FFFF_FFFF;
    v = (v | (v << 16)) & 0x0000_FFFF_0000_FFFF;
    v = (v | (v << 8)) & 0x00FF_00FF_00FF_00FF;
    v = (v | (v << 4)) & 0x0F0F_0F0F_0F0F_0F0F;
    v = (v | (v << 2)) & 0x3333_3333_3333_3333;
    v = (v | (v << 1)) & 0x5555_5555_5555_5555;
    v
}

/// Decode a Morton code back into (x, y) coordinates.
#[must_use]
pub fn morton_decode(code: u64) -> (u32, u32) {
    let x = compact_bits(code) as u32;
    let y = compact_bits(code >> 1) as u32;
    (x, y)
}

/// Compact even bits of a 64-bit value into the low 32 bits.
/// Inverse of `interleave_bits`.
fn compact_bits(mut v: u64) -> u64 {
    v &= 0x5555_5555_5555_5555;
    v = (v | (v >> 1)) & 0x3333_3333_3333_3333;
    v = (v | (v >> 2)) & 0x0F0F_0F0F_0F0F_0F0F;
    v = (v | (v >> 4)) & 0x00FF_00FF_00FF_00FF;
    v = (v | (v >> 8)) & 0x0000_FFFF_0000_FFFF;
    v = (v | (v >> 16)) & 0x0000_0000_FFFF_FFFF;
    v
}

/// Compute Morton code for floating-point coordinates.
///
/// Maps `(x, y)` from the bounding box `[min_x, max_x] × [min_y, max_y]` to
/// `[0, 2^resolution - 1]` integer grid, then computes the Morton code.
///
/// `resolution` controls the grid granularity (default: 16 = 65536 cells per axis).
#[must_use]
pub fn morton_code_f64(
    x: f64,
    y: f64,
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
    resolution: u32,
) -> u64 {
    let range_x = (max_x - min_x).max(f64::MIN_POSITIVE);
    let range_y = (max_y - min_y).max(f64::MIN_POSITIVE);
    let max_val = (1_u64 << resolution) - 1;

    let ix = (((x - min_x) / range_x) * max_val as f64).clamp(0.0, max_val as f64) as u32;
    let iy = (((y - min_y) / range_y) * max_val as f64).clamp(0.0, max_val as f64) as u32;

    morton_code(ix, iy)
}

/// Compute the Morton-order permutation for a set of 2D positions.
///
/// Returns a permutation array `perm` such that `positions[perm[i]]` is the i-th
/// element in Morton (Z-order) traversal.
#[must_use]
pub fn morton_order(positions: &[(f64, f64)]) -> Vec<usize> {
    if positions.is_empty() {
        return Vec::new();
    }

    let (min_x, max_x, min_y, max_y) = bounding_box(positions);

    let mut indexed: Vec<(u64, usize)> = positions
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let code = morton_code_f64(x, y, min_x, max_x, min_y, max_y, 16);
            (code, i)
        })
        .collect();

    // Sort by Morton code, breaking ties by original index for determinism.
    indexed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    indexed.iter().map(|&(_, idx)| idx).collect()
}

/// Compute the bounding box of a set of 2D positions.
fn bounding_box(positions: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;

    for &(x, y) in positions {
        if x < min_x {
            min_x = x;
        }
        if x > max_x {
            max_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if y > max_y {
            max_y = y;
        }
    }

    (min_x, max_x, min_y, max_y)
}

/// A blocked edge group for rank-based traversal.
///
/// Groups edges by their source node's rank, so that crossing minimization
/// can iterate over edges within a rank band without cache misses.
#[derive(Debug, Clone)]
pub struct BlockedEdgeGroups {
    /// For each rank: indices into the original edge list.
    pub rank_groups: Vec<Vec<usize>>,
    /// Total number of ranks.
    pub num_ranks: usize,
}

impl BlockedEdgeGroups {
    /// Build blocked edge groups from edges with source ranks.
    ///
    /// `edge_source_ranks[i]` = the rank of the source node of edge i.
    #[must_use]
    pub fn from_ranks(edge_source_ranks: &[usize], num_ranks: usize) -> Self {
        let mut rank_groups = vec![Vec::new(); num_ranks];

        for (edge_idx, &rank) in edge_source_ranks.iter().enumerate() {
            if rank < num_ranks {
                rank_groups[rank].push(edge_idx);
            }
        }

        Self {
            rank_groups,
            num_ranks,
        }
    }

    /// Get edge indices for a given rank.
    #[must_use]
    pub fn edges_for_rank(&self, rank: usize) -> &[usize] {
        if rank < self.rank_groups.len() {
            &self.rank_groups[rank]
        } else {
            &[]
        }
    }

    /// Total number of edges across all groups.
    #[must_use]
    pub fn total_edges(&self) -> usize {
        self.rank_groups.iter().map(Vec::len).sum()
    }
}

/// Van Emde Boas (vEB) layout for a complete binary tree.
///
/// Reorders tree nodes so that recursive halves of the tree are stored
/// contiguously, ensuring that any subtree of size M fits in O(M/B) cache lines.
///
/// Input: `n` nodes in level-order (BFS) of a complete binary tree.
/// Output: permutation array for vEB layout order.
#[must_use]
pub fn veb_layout_order(n: usize) -> Vec<usize> {
    if n == 0 {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(n);
    veb_recursive(0, n, &mut result);
    result
}

/// Recursively compute vEB layout.
fn veb_recursive(start: usize, count: usize, result: &mut Vec<usize>) {
    if count == 0 {
        return;
    }
    if count == 1 {
        result.push(start);
        return;
    }

    // Find the height of the tree.
    let height = (usize::BITS - count.leading_zeros()) as usize;
    let half_height = height / 2;

    // Top tree has 2^half_height - 1 nodes.
    // Guard against shift overflow for very large inputs.
    let top_size = if half_height >= usize::BITS as usize {
        count
    } else {
        ((1_usize << half_height) - 1).min(count)
    };

    // Layout the top tree first.
    veb_recursive(start, top_size, result);

    // Layout each bottom subtree.
    let remaining = count - top_size;
    if remaining == 0 {
        return;
    }

    let height_diff = height - half_height;
    let bottom_tree_size = if height_diff >= usize::BITS as usize {
        remaining // Single giant subtree as fallback
    } else {
        (1_usize << height_diff) - 1
    };
    let num_bottom_trees = if bottom_tree_size == 0 {
        0
    } else {
        remaining.div_ceil(bottom_tree_size)
    };

    let mut offset = start + top_size;
    for _ in 0..num_bottom_trees {
        let this_size = bottom_tree_size.min(count.saturating_sub(offset - start));
        if this_size == 0 {
            break;
        }
        veb_recursive(offset, this_size, result);
        offset += this_size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Morton code tests --

    #[test]
    fn morton_code_zero() {
        assert_eq!(morton_code(0, 0), 0);
    }

    #[test]
    fn morton_code_simple() {
        // x=1 (0b1), y=0 → interleave → 0b01 = 1
        assert_eq!(morton_code(1, 0), 1);
        // x=0, y=1 → interleave → 0b10 = 2
        assert_eq!(morton_code(0, 1), 2);
        // x=1, y=1 → interleave → 0b11 = 3
        assert_eq!(morton_code(1, 1), 3);
    }

    #[test]
    fn morton_roundtrip() {
        let test_cases = [(0, 0), (1, 0), (0, 1), (1, 1), (255, 255), (1000, 2000)];
        for (x, y) in test_cases {
            let code = morton_code(x, y);
            let (dx, dy) = morton_decode(code);
            assert_eq!((dx, dy), (x, y), "Morton roundtrip failed for ({x}, {y})");
        }
    }

    #[test]
    fn morton_preserves_locality() {
        // Close points should have close Morton codes.
        let c1 = morton_code(10, 10);
        let c2 = morton_code(11, 10);
        let c3 = morton_code(1000, 1000);

        // c1 and c2 should be closer than c1 and c3.
        let diff_close = c1.abs_diff(c2);
        let diff_far = c1.abs_diff(c3);
        assert!(
            diff_close < diff_far,
            "Close points should have closer Morton codes"
        );
    }

    #[test]
    fn morton_code_f64_maps_range() {
        let code_min = morton_code_f64(0.0, 0.0, 0.0, 100.0, 0.0, 100.0, 16);
        let code_max = morton_code_f64(100.0, 100.0, 0.0, 100.0, 0.0, 100.0, 16);

        assert_eq!(code_min, morton_code(0, 0));
        assert_eq!(code_max, morton_code(65535, 65535));
    }

    #[test]
    fn morton_code_f64_clamping() {
        // Out-of-range values should be clamped.
        let code = morton_code_f64(-10.0, 200.0, 0.0, 100.0, 0.0, 100.0, 16);
        // x clamped to 0, y clamped to 65535
        assert_eq!(code, morton_code(0, 65535));
    }

    // -- Morton ordering tests --

    #[test]
    fn morton_order_empty() {
        assert!(morton_order(&[]).is_empty());
    }

    #[test]
    fn morton_order_preserves_all_indices() {
        let positions = vec![(50.0, 50.0), (0.0, 0.0), (100.0, 100.0), (25.0, 75.0)];
        let order = morton_order(&positions);

        assert_eq!(order.len(), positions.len());
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1, 2, 3]);
    }

    #[test]
    fn morton_order_deterministic() {
        let positions = vec![
            (10.0, 20.0),
            (30.0, 40.0),
            (50.0, 60.0),
            (70.0, 80.0),
            (90.0, 10.0),
        ];
        let o1 = morton_order(&positions);
        let o2 = morton_order(&positions);
        assert_eq!(o1, o2, "Morton order should be deterministic");
    }

    #[test]
    fn morton_order_groups_nearby_points() {
        // Two clusters: (0-10, 0-10) and (1000-1010, 1000-1010)
        let positions = vec![
            (5.0, 5.0),       // cluster A
            (1005.0, 1005.0), // cluster B
            (3.0, 7.0),       // cluster A
            (1008.0, 1002.0), // cluster B
        ];
        let order = morton_order(&positions);

        // Cluster A members (indices 0 and 2) should be adjacent in the order.
        let pos_0 = order.iter().position(|&x| x == 0).unwrap();
        let pos_2 = order.iter().position(|&x| x == 2).unwrap();
        let pos_1 = order.iter().position(|&x| x == 1).unwrap();

        assert!(
            (pos_0 as i32 - pos_2 as i32).unsigned_abs() <= 1,
            "Cluster A members should be adjacent, got positions {pos_0} and {pos_2}"
        );
        assert!(
            (pos_0 as i32 - pos_1 as i32).unsigned_abs() > 1,
            "Different clusters should not be adjacent"
        );
    }

    // -- Blocked edge groups tests --

    #[test]
    fn blocked_edges_empty() {
        let groups = BlockedEdgeGroups::from_ranks(&[], 0);
        assert_eq!(groups.total_edges(), 0);
    }

    #[test]
    fn blocked_edges_groups_by_rank() {
        // 5 edges with source ranks: [0, 1, 0, 2, 1]
        let ranks = [0, 1, 0, 2, 1];
        let groups = BlockedEdgeGroups::from_ranks(&ranks, 3);

        assert_eq!(groups.edges_for_rank(0), &[0, 2]);
        assert_eq!(groups.edges_for_rank(1), &[1, 4]);
        assert_eq!(groups.edges_for_rank(2), &[3]);
        assert_eq!(groups.total_edges(), 5);
    }

    #[test]
    fn blocked_edges_out_of_range_rank() {
        let groups = BlockedEdgeGroups::from_ranks(&[0, 1], 3);
        assert!(groups.edges_for_rank(99).is_empty());
    }

    // -- vEB layout tests --

    #[test]
    fn veb_layout_empty() {
        assert!(veb_layout_order(0).is_empty());
    }

    #[test]
    fn veb_layout_single() {
        assert_eq!(veb_layout_order(1), vec![0]);
    }

    #[test]
    fn veb_layout_small() {
        let order = veb_layout_order(7);
        assert_eq!(order.len(), 7);
        // All indices 0..7 should appear exactly once.
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn veb_layout_preserves_all_indices() {
        for n in [3, 7, 15, 31, 63] {
            let order = veb_layout_order(n);
            assert_eq!(order.len(), n);
            let mut sorted = order.clone();
            sorted.sort();
            let expected: Vec<usize> = (0..n).collect();
            assert_eq!(
                sorted, expected,
                "vEB layout for n={n} should be a permutation"
            );
        }
    }
}
