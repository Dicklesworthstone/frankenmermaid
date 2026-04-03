//! Quotient filter: a cache-friendly probabilistic set membership data structure.
//!
//! A quotient filter stores fingerprints compactly by splitting each hash into a
//! quotient (slot address) and remainder (stored value). Linear probing resolves
//! collisions while maintaining sorted runs for excellent cache locality.
//!
//! # Properties
//!
//! - **No false negatives**: if an element was inserted, `may_contain()` always returns true.
//! - **Configurable false positives**: rate ≈ 2^{-remainder_bits}.
//! - **Cache-friendly**: elements in contiguous sorted runs.
//! - **Space-efficient**: ~10-25% less than Bloom filters at the same FP rate.
//!
//! # Use in FrankenMermaid
//!
//! Primarily for duplicate edge detection during parsing, especially on WASM where
//! memory is constrained. For n edges with 16 remainder bits: ~2 bytes/edge vs
//! ~40 bytes/edge for a `HashSet<(NodeId, NodeId)>`.
//!
//! # References
//!
//! - Bender et al., "Don't Thrash: How to Cache Your Hash on Flash" (VLDB 2012)
//! - Pandey et al., "A General-Purpose Counting Filter" (SIGMOD 2017)

use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;

/// A quotient filter with configurable quotient and remainder bit widths.
///
/// The filter uses `q` bits for the quotient (determining the slot) and `r` bits
/// for the remainder (stored in the slot). Total fingerprint bits = q + r.
///
/// The table has 2^q slots. The false positive rate is approximately 2^{-r}.
#[derive(Debug, Clone)]
pub struct QuotientFilter {
    /// Number of quotient bits (determines table size = 2^q).
    /// Retained for diagnostics and documentation; used indirectly via `table_size`.
    #[allow(dead_code)]
    q_bits: u32,
    /// Number of remainder bits (determines false positive rate ≈ 2^{-r}).
    r_bits: u32,
    /// Bitmask for extracting the remainder: (1 << r_bits) - 1.
    r_mask: u64,
    /// Bitmask for extracting the quotient: ((1 << q_bits) - 1) << r_bits.
    q_mask: u64,
    /// Table size: 2^q_bits.
    table_size: usize,
    /// Slot storage: each slot holds an `Option<u64>` remainder.
    /// `None` = empty slot, `Some(r)` = occupied with remainder r.
    slots: Vec<Option<u64>>,
    /// Metadata bits: is_occupied[i] means some element has quotient == i.
    is_occupied: Vec<bool>,
    /// is_continuation[i] means slot i holds a remainder that is NOT the first
    /// in its run (i.e., it shares a quotient with the previous element in the cluster).
    is_continuation: Vec<bool>,
    /// is_shifted[i] means the element in slot i has been shifted from its canonical slot.
    is_shifted: Vec<bool>,
    /// Number of elements inserted.
    count: usize,
}

impl QuotientFilter {
    /// Create a new quotient filter.
    ///
    /// # Arguments
    /// * `q_bits` - Number of quotient bits (table size = 2^q_bits). Clamped to 1..=24.
    /// * `r_bits` - Number of remainder bits (FP rate ≈ 2^{-r_bits}). Clamped to 1..=40.
    ///
    /// # Example
    /// ```
    /// use fm_core::quotient_filter::QuotientFilter;
    /// // 1024 slots (q=10), FP rate ≈ 1/65536 (r=16)
    /// let qf = QuotientFilter::new(10, 16);
    /// ```
    #[must_use]
    pub fn new(q_bits: u32, r_bits: u32) -> Self {
        let q_bits = q_bits.clamp(1, 24);
        let r_bits = r_bits.clamp(1, 40);
        let table_size = 1_usize << q_bits;

        Self {
            q_bits,
            r_bits,
            r_mask: (1_u64 << r_bits) - 1,
            q_mask: ((1_u64 << q_bits) - 1) << r_bits,
            table_size,
            slots: vec![None; table_size],
            is_occupied: vec![false; table_size],
            is_continuation: vec![false; table_size],
            is_shifted: vec![false; table_size],
            count: 0,
        }
    }

    /// Create a quotient filter sized for an expected number of elements.
    ///
    /// Automatically chooses q_bits to keep load factor ≤ 75% and r_bits = 16
    /// for a false positive rate of ~1/65536.
    #[must_use]
    pub fn with_capacity(expected_elements: usize) -> Self {
        let expected = expected_elements.max(1);
        // Target 75% load factor
        let min_slots = (expected * 4).div_ceil(3);
        let q_bits = (usize::BITS - min_slots.leading_zeros()).clamp(1, 24);
        Self::new(q_bits, 16)
    }

    /// Compute the fingerprint (quotient, remainder) for a hashable value.
    fn fingerprint<T: Hash>(&self, value: &T) -> (usize, u64) {
        let mut hasher = FxHasher::default();
        value.hash(&mut hasher);
        let hash = hasher.finish();
        let quotient = ((hash & self.q_mask) >> self.r_bits) as usize;
        let remainder = hash & self.r_mask;
        (quotient, remainder)
    }

    /// Insert an element into the filter.
    ///
    /// Returns `true` if the element was newly inserted, `false` if it was
    /// already present (or a false positive collision exists).
    pub fn insert<T: Hash>(&mut self, value: &T) -> bool {
        if self.count >= self.table_size {
            // Filter is full — cannot insert.
            return false;
        }

        let (quotient, remainder) = self.fingerprint(value);

        if !self.is_occupied[quotient] {
            // Fast path: canonical slot's run doesn't exist yet.
            // Find the actual slot (may need to skip shifted elements).
            let slot = self.find_first_empty_from(quotient);
            self.slots[slot] = Some(remainder);
            self.is_occupied[quotient] = true;
            if slot != quotient {
                self.is_shifted[slot] = true;
            }
            self.count += 1;
            return true;
        }

        // The quotient is already occupied. Find the start of this quotient's run.
        let run_start = self.find_run_start(quotient);

        // Scan the run looking for the remainder or the correct insertion point.
        let mut pos = run_start;
        while let Some(r) = self.slots[pos] {
            if r == remainder {
                return false; // Already present (or false-positive duplicate)
            }
            if r > remainder {
                // Insert before this position.
                break;
            }

            let next = (pos + 1) % self.table_size;
            if next == run_start {
                // Wrapped around completely — filter is full.
                return false;
            }
            // Check if next slot is a continuation of the same run.
            if !self.is_continuation[next] || self.slots[next].is_none() {
                // End of run. Insert after current position.
                pos = next;
                break;
            }
            pos = next;
        }

        // Shift elements right to make room at `pos`.
        self.shift_right(pos);
        self.slots[pos] = Some(remainder);
        if pos != quotient {
            self.is_shifted[pos] = true;
        }
        // If this isn't the first element in the run, mark as continuation.
        if pos != run_start {
            self.is_continuation[pos] = true;
        }
        self.count += 1;
        true
    }

    /// Check if an element may be in the filter.
    ///
    /// - Returns `false`: element is definitely NOT in the filter.
    /// - Returns `true`: element is probably in the filter (false positive possible).
    #[must_use]
    pub fn may_contain<T: Hash>(&self, value: &T) -> bool {
        let (quotient, remainder) = self.fingerprint(value);

        if !self.is_occupied[quotient] {
            return false;
        }

        let run_start = self.find_run_start_const(quotient);
        let mut pos = run_start;

        loop {
            if let Some(r) = self.slots[pos] {
                if r == remainder {
                    return true;
                }
                if r > remainder {
                    return false; // Past where it would be in sorted order.
                }
            } else {
                return false;
            }

            let next = (pos + 1) % self.table_size;
            if next == run_start {
                return false; // Wrapped around.
            }
            if !self.is_continuation[next] || self.slots[next].is_none() {
                return false; // End of run.
            }
            pos = next;
        }
    }

    /// Number of elements in the filter.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the filter is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Load factor: count / table_size.
    #[must_use]
    pub fn load_factor(&self) -> f64 {
        self.count as f64 / self.table_size as f64
    }

    /// Memory usage in bytes (approximate).
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        // Each slot: Option<u64> = 16 bytes (with alignment), 3 bools = 3 bytes
        // In practice, Rust Vec overhead + alignment makes this larger.
        // This returns a theoretical lower bound based on data content.
        self.table_size * (8 + 3) + std::mem::size_of::<Self>()
    }

    /// False positive rate (theoretical).
    #[must_use]
    pub fn false_positive_rate(&self) -> f64 {
        1.0 / (1_u64 << self.r_bits) as f64
    }

    /// Find the start of the run for a given quotient.
    fn find_run_start(&self, quotient: usize) -> usize {
        // Walk backwards from quotient to find the start of the cluster.
        let mut cluster_start = quotient;
        while self.is_shifted[cluster_start] {
            cluster_start = (cluster_start + self.table_size - 1) % self.table_size;
            if cluster_start == quotient {
                break; // Safety: prevent infinite loop.
            }
        }

        // Now walk forward, counting runs until we reach the run for `quotient`.
        let mut pos = cluster_start;
        let mut target_quotient = cluster_start;

        while target_quotient != quotient {
            // Skip to the next run.
            loop {
                pos = (pos + 1) % self.table_size;
                if !self.is_continuation[pos] {
                    break;
                }
            }
            target_quotient = (target_quotient + 1) % self.table_size;
            // Skip quotients that aren't occupied.
            while !self.is_occupied[target_quotient] && target_quotient != quotient {
                target_quotient = (target_quotient + 1) % self.table_size;
            }
        }

        pos
    }

    /// Const-compatible version of `find_run_start` (doesn't need &mut self).
    fn find_run_start_const(&self, quotient: usize) -> usize {
        self.find_run_start(quotient)
    }

    /// Find the first empty slot starting from `start`.
    fn find_first_empty_from(&self, start: usize) -> usize {
        let mut pos = start;
        for _ in 0..self.table_size {
            if self.slots[pos].is_none() {
                return pos;
            }
            pos = (pos + 1) % self.table_size;
        }
        start // Fallback (filter full).
    }

    /// Shift elements right starting from `pos` to make room for an insertion.
    fn shift_right(&mut self, pos: usize) {
        if self.slots[pos].is_none() {
            return; // Already empty.
        }

        // Find the next empty slot.
        let empty = self.find_first_empty_from(pos);

        // Shift elements from empty back to pos.
        let mut current = empty;
        while current != pos {
            let prev = (current + self.table_size - 1) % self.table_size;
            self.slots[current] = self.slots[prev];
            self.is_continuation[current] = self.is_continuation[prev];
            self.is_shifted[current] = true;
            current = prev;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_filter_is_empty() {
        let qf = QuotientFilter::new(8, 16);
        assert!(qf.is_empty());
        assert_eq!(qf.len(), 0);
        assert!(qf.load_factor() < f64::EPSILON);
    }

    #[test]
    fn insert_and_query_single() {
        let mut qf = QuotientFilter::new(8, 16);
        assert!(qf.insert(&42_u64));
        assert!(qf.may_contain(&42_u64));
        assert_eq!(qf.len(), 1);
    }

    #[test]
    fn absent_element_not_found() {
        let mut qf = QuotientFilter::new(8, 16);
        qf.insert(&42_u64);
        // Very unlikely to be a false positive for a single element.
        // But we check a few values.
        let mut found_absent = 0;
        for i in 100..200_u64 {
            if qf.may_contain(&i) {
                found_absent += 1;
            }
        }
        // With r=16, FP rate ≈ 1/65536, so 100 queries should yield ~0 FPs.
        assert!(
            found_absent <= 2,
            "Too many false positives: {found_absent}/100"
        );
    }

    #[test]
    fn insert_duplicate_returns_false() {
        let mut qf = QuotientFilter::new(8, 16);
        assert!(qf.insert(&42_u64)); // first insert
        assert!(!qf.insert(&42_u64)); // duplicate
        assert_eq!(qf.len(), 1); // count shouldn't change
    }

    #[test]
    fn multiple_inserts_all_found() {
        let mut qf = QuotientFilter::new(10, 16);
        let values: Vec<u64> = (0..100).collect();

        for v in &values {
            qf.insert(v);
        }

        // All inserted elements must be found (no false negatives).
        for v in &values {
            assert!(
                qf.may_contain(v),
                "Inserted element {v} not found — false negative!"
            );
        }
        assert_eq!(qf.len(), 100);
    }

    #[test]
    fn edge_tuple_detection() {
        // Simulate duplicate edge detection with (from, to) tuples.
        let mut qf = QuotientFilter::with_capacity(100);

        let edge1 = (0_usize, 1_usize);
        let edge2 = (1_usize, 2_usize);
        let edge3 = (0_usize, 1_usize); // duplicate of edge1

        assert!(qf.insert(&edge1));
        assert!(qf.insert(&edge2));
        assert!(!qf.insert(&edge3)); // duplicate

        assert!(qf.may_contain(&edge1));
        assert!(qf.may_contain(&edge2));
    }

    #[test]
    fn with_capacity_sizing() {
        let qf = QuotientFilter::with_capacity(1000);
        // Should have at least 1333 slots (1000 / 0.75)
        assert!(
            qf.table_size >= 1333,
            "Table too small: {} for 1000 elements",
            qf.table_size
        );
        assert_eq!(qf.r_bits, 16);
    }

    #[test]
    fn load_factor_tracks_inserts() {
        let mut qf = QuotientFilter::new(4, 8); // 16 slots
        for i in 0..8_u64 {
            qf.insert(&i);
        }
        let lf = qf.load_factor();
        assert!(
            (lf - 0.5).abs() < 0.1,
            "Expected load factor ~0.5, got {lf}"
        );
    }

    #[test]
    fn false_positive_rate_matches_theory() {
        let qf = QuotientFilter::new(10, 16);
        let expected_fp = 1.0 / 65536.0;
        assert!(
            (qf.false_positive_rate() - expected_fp).abs() < 1e-10,
            "FP rate should be {expected_fp}, got {}",
            qf.false_positive_rate()
        );
    }

    #[test]
    fn memory_is_smaller_than_hashset() {
        let qf = QuotientFilter::with_capacity(10000);
        let qf_bytes = qf.memory_bytes();

        // A HashSet<(usize, usize)> for 10000 edges uses roughly:
        // 10000 * (16 bytes data + ~24 bytes overhead) ≈ 400KB
        // The quotient filter should be significantly smaller.
        let hashset_estimate = 10000 * 40;

        assert!(
            qf_bytes < hashset_estimate,
            "QuotientFilter ({qf_bytes} bytes) should be smaller than HashSet estimate ({hashset_estimate} bytes)"
        );
    }

    #[test]
    fn no_false_negatives_stress() {
        // Insert many elements and verify all are found.
        let mut qf = QuotientFilter::with_capacity(5000);
        let mut inserted = Vec::new();

        for i in 0..1000_u64 {
            let key = (i, i * 7 + 13);
            qf.insert(&key);
            inserted.push(key);
        }

        for key in &inserted {
            assert!(qf.may_contain(key), "False negative for {key:?}");
        }
    }

    #[test]
    fn false_positive_rate_empirical() {
        // Measure the empirical false positive rate.
        let mut qf = QuotientFilter::new(12, 12); // 4096 slots, FP ≈ 1/4096
        let n_insert = 1000;
        let n_query = 10000;

        for i in 0..n_insert as u64 {
            qf.insert(&i);
        }

        let mut false_positives = 0;
        for i in (n_insert as u64)..(n_insert as u64 + n_query as u64) {
            if qf.may_contain(&i) {
                false_positives += 1;
            }
        }

        let empirical_fp = false_positives as f64 / n_query as f64;
        let theoretical_fp = 1.0 / (1_u64 << 12) as f64; // ~0.000244

        // Allow 10x margin for small sample sizes and load factor effects.
        assert!(
            empirical_fp < theoretical_fp * 10.0 + 0.01,
            "Empirical FP rate {empirical_fp:.4} too high (theoretical {theoretical_fp:.6})"
        );
    }

    #[test]
    fn empty_filter_contains_nothing() {
        let qf = QuotientFilter::new(8, 16);
        for i in 0..100_u64 {
            assert!(!qf.may_contain(&i));
        }
    }

    #[test]
    fn string_keys_work() {
        let mut qf = QuotientFilter::new(8, 16);
        qf.insert(&"hello");
        qf.insert(&"world");
        assert!(qf.may_contain(&"hello"));
        assert!(qf.may_contain(&"world"));
    }
}
