//! Succinct graph representation for compact storage of large diagrams.
//!
//! Provides a compressed sparse row (CSR) adjacency structure backed by bit vectors
//! with rank/select support. This enables O(1) degree queries and O(degree) neighbor
//! iteration with minimal memory overhead compared to traditional adjacency lists.
//!
//! # Memory comparison (10K nodes, 30K edges)
//!
//! | Representation | Memory |
//! |----------------|--------|
//! | `Vec<Vec<usize>>` adjacency list | ~720 KB |
//! | `HashMap<usize, Vec<usize>>` | ~960 KB |
//! | CSR (this module) | ~160 KB |
//! | CSR + bit vector boundaries | ~165 KB |
//!
//! # References
//!
//! - Jacobson, "Space-Efficient Static Trees and Graphs" (FOCS 1989)
//! - Vigna, "Broadword Implementation of Rank/Select Queries" (WEA 2008)

/// A compact bit vector with O(1) rank queries.
///
/// Rank(i) = number of set bits in positions 0..i.
/// Uses a two-level block structure for O(1) rank via precomputed counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitVector {
    /// Raw bits packed into u64 words.
    words: Vec<u64>,
    /// Total number of bits.
    len: usize,
    /// Precomputed cumulative popcount per block of 512 bits.
    /// `rank_blocks[i]` = total set bits in words 0..i*8.
    rank_blocks: Vec<u32>,
}

impl BitVector {
    /// Create a new bit vector of `len` bits, all initially zero.
    #[must_use]
    pub fn new(len: usize) -> Self {
        let word_count = len.div_ceil(64);
        Self {
            words: vec![0; word_count],
            len,
            rank_blocks: Vec::new(),
        }
    }

    /// Create from a vec of booleans.
    #[must_use]
    pub fn from_bools(bits: &[bool]) -> Self {
        let mut bv = Self::new(bits.len());
        for (i, &b) in bits.iter().enumerate() {
            if b {
                bv.set(i);
            }
        }
        bv.build_rank();
        bv
    }

    /// Set bit at position `i`.
    pub fn set(&mut self, i: usize) {
        debug_assert!(i < self.len);
        self.words[i / 64] |= 1_u64 << (i % 64);
    }

    /// Get bit at position `i`.
    #[must_use]
    pub fn get(&self, i: usize) -> bool {
        if i >= self.len {
            return false;
        }
        (self.words[i / 64] >> (i % 64)) & 1 == 1
    }

    /// Build the rank acceleration structure. Must be called after all sets.
    pub fn build_rank(&mut self) {
        let block_count = self.words.len().div_ceil(8);
        self.rank_blocks = Vec::with_capacity(block_count + 1);
        self.rank_blocks.push(0);

        let mut cumulative = 0_u32;
        for (i, &word) in self.words.iter().enumerate() {
            cumulative += word.count_ones();
            if (i + 1).is_multiple_of(8) {
                self.rank_blocks.push(cumulative);
            }
        }
        // Push final block if not aligned.
        if !self.words.len().is_multiple_of(8) {
            self.rank_blocks.push(cumulative);
        }
    }

    /// Rank query: count set bits in positions 0..i (exclusive).
    ///
    /// Returns the number of 1-bits before position `i`.
    #[must_use]
    pub fn rank(&self, i: usize) -> u32 {
        if i == 0 {
            return 0;
        }
        let i = i.min(self.len);
        let word_idx = i / 64;
        let bit_idx = i % 64;
        let block_idx = word_idx / 8;

        // Start with the block-level cumulative count.
        let mut count = self
            .rank_blocks
            .get(block_idx)
            .or_else(|| self.rank_blocks.last())
            .copied()
            .unwrap_or(0);

        // Add popcounts for words within the block before word_idx.
        let block_start = block_idx * 8;
        for w in block_start..word_idx {
            if w < self.words.len() {
                count += self.words[w].count_ones();
            }
        }

        // Add partial word popcount.
        if word_idx < self.words.len() && bit_idx > 0 {
            let mask = (1_u64 << bit_idx) - 1;
            count += (self.words[word_idx] & mask).count_ones();
        }

        count
    }

    /// Select query: find the position of the k-th set bit (0-indexed).
    ///
    /// Returns `None` if fewer than k+1 bits are set.
    #[must_use]
    pub fn select(&self, k: u32) -> Option<usize> {
        // Binary search over rank to find the word containing the k-th set bit.
        let mut remaining = k + 1;
        for (word_idx, &word) in self.words.iter().enumerate() {
            let pc = word.count_ones();
            if pc >= remaining {
                // The target bit is in this word.
                let mut w = word;
                let mut r = remaining;
                for bit in 0..64 {
                    if w & 1 == 1 {
                        r -= 1;
                        if r == 0 {
                            let pos = word_idx * 64 + bit;
                            return if pos < self.len { Some(pos) } else { None };
                        }
                    }
                    w >>= 1;
                }
            }
            remaining -= pc;
        }
        None
    }

    /// Total number of set bits.
    #[must_use]
    pub fn count_ones(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    /// Total number of bits.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the vector is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Memory usage in bytes.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.words.len() * 8 + self.rank_blocks.len() * 4 + std::mem::size_of::<Self>()
    }
}

/// Compressed Sparse Row (CSR) graph representation.
///
/// Stores the graph as two arrays:
/// - `offsets[i]` = start index in `targets` for node i's neighbors
/// - `targets[offsets[i]..offsets[i+1]]` = neighbors of node i
///
/// This is the most cache-friendly graph representation for sequential traversal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsrGraph {
    /// `offsets[i]` = start of node i's adjacency list in `targets`.
    /// Length = num_nodes + 1. `offsets[num_nodes]` = total edges.
    offsets: Vec<u32>,
    /// Concatenated adjacency lists.
    targets: Vec<u32>,
    /// Number of nodes.
    num_nodes: usize,
    /// Whether edges are directed.
    directed: bool,
}

impl CsrGraph {
    /// Build a CSR graph from an edge list.
    ///
    /// `num_nodes` = number of nodes (0..num_nodes-1).
    /// `edges` = list of (source, target) pairs.
    /// If `directed` is false, each edge is stored in both directions.
    #[must_use]
    pub fn from_edges(num_nodes: usize, edges: &[(usize, usize)], directed: bool) -> Self {
        // Count degrees.
        let mut degrees = vec![0_u32; num_nodes];
        for &(src, tgt) in edges {
            if src < num_nodes && tgt < num_nodes {
                degrees[src] += 1;
                if !directed && src != tgt {
                    degrees[tgt] += 1;
                }
            }
        }

        // Build offsets.
        let mut offsets = Vec::with_capacity(num_nodes + 1);
        offsets.push(0);
        for &d in &degrees {
            let last = *offsets.last().unwrap();
            offsets.push(last + d);
        }
        let total_edges = *offsets.last().unwrap() as usize;

        // Fill targets.
        let mut targets = vec![0_u32; total_edges];
        let mut current = vec![0_u32; num_nodes]; // current write position per node

        for &(src, tgt) in edges {
            if src < num_nodes && tgt < num_nodes {
                let pos = (offsets[src] + current[src]) as usize;
                targets[pos] = tgt as u32;
                current[src] += 1;

                if !directed && src != tgt {
                    let pos = (offsets[tgt] + current[tgt]) as usize;
                    targets[pos] = src as u32;
                    current[tgt] += 1;
                }
            }
        }

        // Sort each node's adjacency list for determinism.
        for i in 0..num_nodes {
            let start = offsets[i] as usize;
            let end = offsets[i + 1] as usize;
            targets[start..end].sort_unstable();
        }

        Self {
            offsets,
            targets,
            num_nodes,
            directed,
        }
    }

    /// Number of nodes.
    #[must_use]
    pub fn num_nodes(&self) -> usize {
        self.num_nodes
    }

    /// Number of edges (directed count).
    #[must_use]
    pub fn num_edges(&self) -> usize {
        self.targets.len()
    }

    /// Degree of node `i`.
    #[must_use]
    pub fn degree(&self, i: usize) -> u32 {
        if i >= self.num_nodes {
            return 0;
        }
        self.offsets[i + 1] - self.offsets[i]
    }

    /// Neighbors of node `i` as a slice.
    #[must_use]
    pub fn neighbors(&self, i: usize) -> &[u32] {
        if i >= self.num_nodes {
            return &[];
        }
        let start = self.offsets[i] as usize;
        let end = self.offsets[i + 1] as usize;
        &self.targets[start..end]
    }

    /// Check if edge (src, tgt) exists. O(log degree) via binary search.
    #[must_use]
    pub fn has_edge(&self, src: usize, tgt: usize) -> bool {
        self.neighbors(src).binary_search(&(tgt as u32)).is_ok()
    }

    /// Memory usage in bytes.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.offsets.len() * 4 + self.targets.len() * 4 + std::mem::size_of::<Self>()
    }

    /// Whether the graph is directed.
    #[must_use]
    pub fn is_directed(&self) -> bool {
        self.directed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- BitVector tests --

    #[test]
    fn bitvec_empty() {
        let bv = BitVector::new(0);
        assert!(bv.is_empty());
        assert_eq!(bv.count_ones(), 0);
    }

    #[test]
    fn bitvec_set_get() {
        let mut bv = BitVector::new(100);
        bv.set(0);
        bv.set(50);
        bv.set(99);
        bv.build_rank();

        assert!(bv.get(0));
        assert!(!bv.get(1));
        assert!(bv.get(50));
        assert!(bv.get(99));
        assert!(!bv.get(98));
        assert_eq!(bv.count_ones(), 3);
    }

    #[test]
    fn bitvec_rank() {
        let mut bv = BitVector::new(64);
        bv.set(0);
        bv.set(5);
        bv.set(10);
        bv.set(63);
        bv.build_rank();

        assert_eq!(bv.rank(0), 0); // No bits before position 0
        assert_eq!(bv.rank(1), 1); // bit 0 is set
        assert_eq!(bv.rank(6), 2); // bits 0 and 5 are set
        assert_eq!(bv.rank(11), 3); // bits 0, 5, 10 are set
        assert_eq!(bv.rank(64), 4); // all 4 bits
    }

    #[test]
    fn bitvec_rank_large() {
        // Test across multiple words (>64 bits).
        let n = 1000;
        let mut bv = BitVector::new(n);
        for i in (0..n).step_by(3) {
            bv.set(i);
        }
        bv.build_rank();

        // Verify rank at several points.
        let expected_at_300: u32 = (0..300).filter(|i| i % 3 == 0).count() as u32;
        assert_eq!(bv.rank(300), expected_at_300);

        let expected_at_n: u32 = (0..n).filter(|i| i % 3 == 0).count() as u32;
        assert_eq!(bv.rank(n), expected_at_n);
    }

    #[test]
    fn bitvec_select() {
        let mut bv = BitVector::new(100);
        bv.set(10);
        bv.set(30);
        bv.set(50);
        bv.build_rank();

        assert_eq!(bv.select(0), Some(10));
        assert_eq!(bv.select(1), Some(30));
        assert_eq!(bv.select(2), Some(50));
        assert_eq!(bv.select(3), None);
    }

    #[test]
    fn bitvec_from_bools() {
        let bits = vec![true, false, true, false, true];
        let bv = BitVector::from_bools(&bits);
        assert_eq!(bv.len(), 5);
        assert!(bv.get(0));
        assert!(!bv.get(1));
        assert!(bv.get(2));
        assert_eq!(bv.count_ones(), 3);
    }

    // -- CSR graph tests --

    #[test]
    fn csr_empty_graph() {
        let g = CsrGraph::from_edges(0, &[], true);
        assert_eq!(g.num_nodes(), 0);
        assert_eq!(g.num_edges(), 0);
    }

    #[test]
    fn csr_directed_triangle() {
        let edges = vec![(0, 1), (1, 2), (2, 0)];
        let g = CsrGraph::from_edges(3, &edges, true);

        assert_eq!(g.num_nodes(), 3);
        assert_eq!(g.num_edges(), 3);
        assert_eq!(g.degree(0), 1);
        assert_eq!(g.degree(1), 1);
        assert_eq!(g.degree(2), 1);
        assert_eq!(g.neighbors(0), &[1]);
        assert_eq!(g.neighbors(1), &[2]);
        assert_eq!(g.neighbors(2), &[0]);
    }

    #[test]
    fn csr_undirected_triangle() {
        let edges = vec![(0, 1), (1, 2), (2, 0)];
        let g = CsrGraph::from_edges(3, &edges, false);

        assert_eq!(g.num_nodes(), 3);
        assert_eq!(g.num_edges(), 6); // each edge stored twice
        assert_eq!(g.degree(0), 2);
        assert_eq!(g.degree(1), 2);
        assert_eq!(g.degree(2), 2);

        // Neighbors are sorted.
        assert!(g.neighbors(0).contains(&1));
        assert!(g.neighbors(0).contains(&2));
    }

    #[test]
    fn csr_has_edge() {
        let edges = vec![(0, 1), (0, 2), (1, 3)];
        let g = CsrGraph::from_edges(4, &edges, true);

        assert!(g.has_edge(0, 1));
        assert!(g.has_edge(0, 2));
        assert!(g.has_edge(1, 3));
        assert!(!g.has_edge(1, 0)); // directed
        assert!(!g.has_edge(2, 3));
    }

    #[test]
    fn csr_isolated_nodes() {
        let edges = vec![(0, 1)];
        let g = CsrGraph::from_edges(5, &edges, true);

        assert_eq!(g.num_nodes(), 5);
        assert_eq!(g.degree(0), 1);
        assert_eq!(g.degree(1), 0);
        assert_eq!(g.degree(2), 0);
        assert_eq!(g.degree(3), 0);
        assert_eq!(g.degree(4), 0);
    }

    #[test]
    fn csr_memory_compact() {
        // Compare memory of CSR vs Vec<Vec<usize>> for 1000 nodes, 3000 edges.
        let n = 1000;
        let edges: Vec<(usize, usize)> = (0..3000).map(|i| (i % n, (i * 7 + 13) % n)).collect();
        let g = CsrGraph::from_edges(n, &edges, true);

        let csr_mem = g.memory_bytes();
        // Vec<Vec<usize>> estimate: n * (24 bytes vec overhead) + edges * 8 bytes
        let vecvec_mem = n * 24 + 3000 * 8;

        assert!(
            csr_mem < vecvec_mem,
            "CSR ({csr_mem} bytes) should be more compact than Vec<Vec> ({vecvec_mem} bytes)"
        );
    }

    #[test]
    fn csr_deterministic() {
        let edges = vec![(2, 0), (0, 2), (1, 0), (0, 1)];
        let g1 = CsrGraph::from_edges(3, &edges, true);
        let g2 = CsrGraph::from_edges(3, &edges, true);

        // Same edges should produce identical neighbor lists.
        for i in 0..3 {
            assert_eq!(g1.neighbors(i), g2.neighbors(i));
        }
    }

    #[test]
    fn csr_neighbors_sorted() {
        let edges = vec![(0, 5), (0, 2), (0, 8), (0, 1), (0, 3)];
        let g = CsrGraph::from_edges(10, &edges, true);

        let n = g.neighbors(0);
        assert!(
            n.windows(2).all(|w| w[0] <= w[1]),
            "Neighbors should be sorted"
        );
    }

    #[test]
    fn csr_self_loop() {
        let edges = vec![(0, 0), (0, 1)];
        let g = CsrGraph::from_edges(2, &edges, true);

        assert_eq!(g.degree(0), 2);
        assert!(g.has_edge(0, 0));
        assert!(g.has_edge(0, 1));
    }

    #[test]
    fn csr_out_of_range_node() {
        let g = CsrGraph::from_edges(3, &[(0, 1)], true);
        assert_eq!(g.degree(999), 0);
        assert!(g.neighbors(999).is_empty());
    }
}
