//! Leapfrog Triejoin: worst-case optimal multi-way join over sorted relations.
//!
//! Implements the Leapfrog Triejoin algorithm for efficiently joining multiple
//! sorted relations. This is useful for complex diagram queries like "find all
//! nodes that are both sources of edges to cluster X and targets of edges from
//! cluster Y" — a multi-relation join.
//!
//! # Algorithm
//!
//! Leapfrog Triejoin works by maintaining sorted iterators over each relation
//! and "leapfrogging" — advancing each iterator to at least the maximum current
//! key across all iterators. When all iterators point to the same key, that key
//! is in the join result.
//!
//! For k relations of size n, the join runs in O(n^{k/(k+1)} * k * log n) time,
//! which is worst-case optimal for natural joins.
//!
//! # Example
//!
//! ```ignore
//! use fm_core::leapfrog::{SortedRelation, leapfrog_join};
//!
//! // Edges: (source, target)
//! let edges_from = SortedRelation::new(vec![0, 0, 1, 2, 3]);
//! let edges_to = SortedRelation::new(vec![1, 3, 2, 0, 1]);
//!
//! // Find nodes that are both a source and a target:
//! let sources = SortedRelation::from_slice(&[0, 1, 2, 3]);
//! let targets = SortedRelation::from_slice(&[0, 1, 2, 3]);
//! let both = leapfrog_join(&[&sources, &targets]);
//! // both = [0, 1, 2, 3] — all nodes appear as both source and target
//! ```
//!
//! # References
//!
//! - Veldhuizen, "Leapfrog Triejoin: A Simple, Worst-Case Optimal Join Algorithm" (ICDT 2014)
//! - Ngo et al., "Worst-Case Optimal Join Algorithms" (JACM 2018)

/// A sorted relation (set of keys) supporting leapfrog iteration.
///
/// Keys must be sorted in ascending order with no duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortedRelation {
    keys: Vec<u64>,
}

impl SortedRelation {
    /// Create a new sorted relation from a pre-sorted, deduplicated key vector.
    ///
    /// # Panics
    /// Debug-asserts that keys are sorted and unique.
    #[must_use]
    pub fn new(keys: Vec<u64>) -> Self {
        debug_assert!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "Keys must be sorted and unique"
        );
        Self { keys }
    }

    /// Create from an unsorted slice, sorting and deduplicating.
    #[must_use]
    pub fn from_unsorted(keys: &[u64]) -> Self {
        let mut sorted = keys.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        Self { keys: sorted }
    }

    /// Create from a sorted slice (no copy if already sorted).
    #[must_use]
    pub fn from_slice(keys: &[u64]) -> Self {
        Self::new(keys.to_vec())
    }

    /// Number of keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the relation is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Check if a key exists (binary search).
    #[must_use]
    pub fn contains(&self, key: u64) -> bool {
        self.keys.binary_search(&key).is_ok()
    }

    /// Get the underlying sorted keys.
    #[must_use]
    pub fn keys(&self) -> &[u64] {
        &self.keys
    }

    /// Find the first key >= `target` (seek operation).
    /// Returns the index, or `self.len()` if no key >= target.
    #[must_use]
    pub fn seek(&self, target: u64) -> usize {
        match self.keys.binary_search(&target) {
            Ok(idx) | Err(idx) => idx,
        }
    }
}

/// A leapfrog iterator over a `SortedRelation`.
///
/// Supports `seek(target)` to advance to the first key >= target,
/// and `next()` to advance to the next key.
#[derive(Debug)]
pub struct LeapfrogIter<'a> {
    relation: &'a SortedRelation,
    pos: usize,
}

impl<'a> LeapfrogIter<'a> {
    /// Create a new iterator at the beginning of the relation.
    #[must_use]
    pub fn new(relation: &'a SortedRelation) -> Self {
        Self { relation, pos: 0 }
    }

    /// Current key, or `None` if exhausted.
    #[must_use]
    pub fn current(&self) -> Option<u64> {
        self.relation.keys.get(self.pos).copied()
    }

    /// Whether the iterator is exhausted.
    #[must_use]
    pub fn at_end(&self) -> bool {
        self.pos >= self.relation.len()
    }

    /// Advance to the first key >= `target`.
    /// Returns the key found, or `None` if exhausted.
    pub fn seek(&mut self, target: u64) -> Option<u64> {
        // Binary search from current position forward.
        let remaining = &self.relation.keys[self.pos..];
        let offset = match remaining.binary_search(&target) {
            Ok(idx) | Err(idx) => idx,
        };
        self.pos += offset;
        self.current()
    }

    /// Advance to the next key.
    pub fn advance(&mut self) -> Option<u64> {
        if self.pos < self.relation.len() {
            self.pos += 1;
        }
        self.current()
    }
}

/// Perform a leapfrog join over multiple sorted relations.
///
/// Returns the intersection of all relations: keys that appear in every relation.
///
/// Uses the leapfrog protocol:
/// 1. Sort iterators by their current key.
/// 2. If all iterators point to the same key → emit it.
/// 3. Otherwise, seek the smallest iterator to the maximum current key.
/// 4. Repeat until any iterator is exhausted.
///
/// Time complexity: O(n * k * log n) where n = max relation size, k = number of relations.
#[must_use]
pub fn leapfrog_join(relations: &[&SortedRelation]) -> Vec<u64> {
    if relations.is_empty() {
        return Vec::new();
    }
    if relations.len() == 1 {
        return relations[0].keys.clone();
    }

    // Check for empty relations (intersection is empty).
    if relations.iter().any(|r| r.is_empty()) {
        return Vec::new();
    }

    let mut iters: Vec<LeapfrogIter<'_>> = relations.iter().map(|r| LeapfrogIter::new(r)).collect();
    let mut result = Vec::new();

    // Initialize: find the maximum starting key.
    let mut max_key = iters
        .iter()
        .filter_map(|it| it.current())
        .max()
        .unwrap_or(0);

    loop {
        // Seek all iterators to at least max_key.
        let mut all_equal = true;
        let mut new_max = max_key;

        for iter in &mut iters {
            match iter.seek(max_key) {
                Some(key) => {
                    if key != max_key {
                        all_equal = false;
                        if key > new_max {
                            new_max = key;
                        }
                    }
                }
                None => return result, // Iterator exhausted.
            }
        }

        if all_equal {
            // All iterators point to the same key — it's in the join.
            result.push(max_key);

            // Advance all iterators past this key.
            for iter in &mut iters {
                iter.advance();
            }

            // Find new max.
            match iters.iter().filter_map(|it| it.current()).max() {
                Some(key) => max_key = key,
                None => return result, // All exhausted.
            }
        } else {
            max_key = new_max;
        }
    }
}

/// Perform a leapfrog anti-join: keys in `base` that are NOT in any of `exclude`.
///
/// Useful for "find nodes not connected to cluster X" queries.
#[must_use]
pub fn leapfrog_anti_join(base: &SortedRelation, exclude: &[&SortedRelation]) -> Vec<u64> {
    if exclude.is_empty() {
        return base.keys.clone();
    }

    // Use union of all exclude sets: a key is excluded if it appears in ANY exclude relation.
    let excluded = leapfrog_union(exclude);
    let excluded_set = SortedRelation::new(excluded);

    base.keys
        .iter()
        .filter(|&&k| !excluded_set.contains(k))
        .copied()
        .collect()
}

/// Perform a leapfrog union: keys that appear in ANY relation.
///
/// Merges all sorted relations into a single sorted, deduplicated result.
#[must_use]
pub fn leapfrog_union(relations: &[&SortedRelation]) -> Vec<u64> {
    if relations.is_empty() {
        return Vec::new();
    }
    if relations.len() == 1 {
        return relations[0].keys.clone();
    }

    // k-way merge of sorted sequences.
    let mut result = Vec::new();
    let mut iters: Vec<LeapfrogIter<'_>> = relations.iter().map(|r| LeapfrogIter::new(r)).collect();

    loop {
        // Find minimum current key across all non-exhausted iterators.
        let min_key = iters.iter().filter_map(|it| it.current()).min();

        let Some(key) = min_key else {
            break;
        };

        result.push(key);

        // Advance all iterators pointing to this key.
        for iter in &mut iters {
            if iter.current() == Some(key) {
                iter.advance();
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_relation_from_unsorted() {
        let r = SortedRelation::from_unsorted(&[5, 3, 1, 3, 2, 1]);
        assert_eq!(r.keys(), &[1, 2, 3, 5]);
    }

    #[test]
    fn sorted_relation_contains() {
        let r = SortedRelation::from_slice(&[1, 3, 5, 7, 9]);
        assert!(r.contains(5));
        assert!(!r.contains(4));
        assert!(!r.contains(0));
        assert!(!r.contains(10));
    }

    #[test]
    fn sorted_relation_seek() {
        let r = SortedRelation::from_slice(&[2, 4, 6, 8, 10]);
        assert_eq!(r.seek(4), 1); // exact match
        assert_eq!(r.seek(5), 2); // next after 5 is 6 at index 2
        assert_eq!(r.seek(1), 0); // before first
        assert_eq!(r.seek(11), 5); // after last
    }

    #[test]
    fn leapfrog_iter_basic() {
        let r = SortedRelation::from_slice(&[1, 3, 5, 7]);
        let mut iter = LeapfrogIter::new(&r);

        assert_eq!(iter.current(), Some(1));
        assert_eq!(iter.seek(3), Some(3));
        assert_eq!(iter.advance(), Some(5));
        assert_eq!(iter.seek(6), Some(7));
        assert_eq!(iter.advance(), None);
        assert!(iter.at_end());
    }

    #[test]
    fn join_two_relations() {
        let a = SortedRelation::from_slice(&[1, 2, 3, 4, 5]);
        let b = SortedRelation::from_slice(&[2, 4, 6, 8]);

        let result = leapfrog_join(&[&a, &b]);
        assert_eq!(result, vec![2, 4]);
    }

    #[test]
    fn join_three_relations() {
        let a = SortedRelation::from_slice(&[1, 2, 3, 4, 5, 6]);
        let b = SortedRelation::from_slice(&[2, 4, 6, 8]);
        let c = SortedRelation::from_slice(&[3, 4, 5, 6, 7]);

        let result = leapfrog_join(&[&a, &b, &c]);
        assert_eq!(result, vec![4, 6]);
    }

    #[test]
    fn join_disjoint() {
        let a = SortedRelation::from_slice(&[1, 2, 3]);
        let b = SortedRelation::from_slice(&[4, 5, 6]);

        let result = leapfrog_join(&[&a, &b]);
        assert!(result.is_empty());
    }

    #[test]
    fn join_identical() {
        let a = SortedRelation::from_slice(&[1, 2, 3]);
        let b = SortedRelation::from_slice(&[1, 2, 3]);

        let result = leapfrog_join(&[&a, &b]);
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn join_single_relation() {
        let a = SortedRelation::from_slice(&[1, 2, 3]);
        let result = leapfrog_join(&[&a]);
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn join_empty() {
        let result: Vec<u64> = leapfrog_join(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn join_with_empty_relation() {
        let a = SortedRelation::from_slice(&[1, 2, 3]);
        let b = SortedRelation::new(vec![]);

        let result = leapfrog_join(&[&a, &b]);
        assert!(result.is_empty());
    }

    #[test]
    fn join_large_relations() {
        let a = SortedRelation::new((0..1000).collect());
        let b = SortedRelation::new((500..1500).collect());
        let c = SortedRelation::new((750..1250).collect());

        let result = leapfrog_join(&[&a, &b, &c]);
        let expected: Vec<u64> = (750..1000).collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn anti_join_basic() {
        let base = SortedRelation::from_slice(&[1, 2, 3, 4, 5]);
        let exclude = SortedRelation::from_slice(&[2, 4]);

        let result = leapfrog_anti_join(&base, &[&exclude]);
        assert_eq!(result, vec![1, 3, 5]);
    }

    #[test]
    fn anti_join_no_exclusion() {
        let base = SortedRelation::from_slice(&[1, 2, 3]);
        let result = leapfrog_anti_join(&base, &[]);
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn union_basic() {
        let a = SortedRelation::from_slice(&[1, 3, 5]);
        let b = SortedRelation::from_slice(&[2, 4, 6]);

        let result = leapfrog_union(&[&a, &b]);
        assert_eq!(result, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn union_overlapping() {
        let a = SortedRelation::from_slice(&[1, 2, 3]);
        let b = SortedRelation::from_slice(&[2, 3, 4]);

        let result = leapfrog_union(&[&a, &b]);
        assert_eq!(result, vec![1, 2, 3, 4]);
    }

    #[test]
    fn union_empty() {
        let result: Vec<u64> = leapfrog_union(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn join_deterministic() {
        let a = SortedRelation::new((0..100).collect());
        let b = SortedRelation::new((50..150).collect());

        let r1 = leapfrog_join(&[&a, &b]);
        let r2 = leapfrog_join(&[&a, &b]);
        assert_eq!(r1, r2);
    }

    #[test]
    fn diagram_query_pattern() {
        // Simulate: "find nodes that are both edge sources AND in subgraph 1"
        let edge_sources = SortedRelation::from_unsorted(&[0, 1, 2, 3, 0, 1]);
        let subgraph_1_members = SortedRelation::from_slice(&[1, 2, 5, 8]);

        let result = leapfrog_join(&[&edge_sources, &subgraph_1_members]);
        assert_eq!(result, vec![1, 2]);
    }
}
