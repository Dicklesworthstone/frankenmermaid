//! Counted proof that a batch actually did the work its timing claims (bd-l7mu, bd-dqkg).
//!
//! # The defect this exists to make impossible
//!
//! A memoised batch is indistinguishable from a fast engine if you only look at the clock.
//! `schema_catalog_25` once reported **16 ns** for a job whose own record claimed 1,663,670 output
//! bytes, and it passed every gate the harness had: output equivalence 25/25, the fm bracket, the
//! median CI, and a balanced-square incumbent null at 1.0120. Iteration 1 rendered; iterations
//! 2..342,682 were cache probes, so the reported p50 was the cost of a cache probe.
//!
//! The rate gate added afterwards (bytes per nanosecond against per-thread store bandwidth) catches
//! that particular shape, but it CANNOT admit an honest reuse row: a memo hit legitimately returns a
//! document it did not recompute, so a truthful incremental measurement lands orders of magnitude
//! above any bandwidth ceiling and is refused for looking exactly like the defect. Raising the
//! ceiling to admit it would delete the gate.
//!
//! The way out is not a better threshold. It is a COUNT: state how many documents were requested
//! and how many were actually computed, and let the row say which kind of measurement it is.
//!
//! # The rule
//!
//! * `computed == requested` — every document was produced. The row may claim engine throughput.
//! * `computed < requested`  — reuse happened. The row is a REUSE measurement and must be read as
//!   one; it may not be compared against a full-computation row, in either direction.
//! * `computed == 0` with `requested > 0` — nothing was computed at all. No timing from this batch
//!   means anything.
//!
//! Nothing here measures or gates by itself; it makes the distinction expressible so a row cannot
//! be silently misfiled as the other kind.

use serde::{Deserialize, Serialize};

/// How much of a batch was actually computed, as opposed to served from reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReuseProof {
    /// Documents the caller asked for.
    pub requested: usize,
    /// Documents actually computed, i.e. those that did NOT come from a memo.
    pub computed: usize,
}

/// What a batch's counts say about the timing taken from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorkClass {
    /// Every requested document was computed. A throughput claim is admissible.
    FullComputation,
    /// Some documents were served from reuse. The row measures reuse, not throughput.
    Reuse { reused: usize },
    /// Nothing was computed. No timing from this batch is meaningful.
    NothingComputed,
    /// The counts are impossible, so they describe no run at all.
    Impossible,
}

impl ReuseProof {
    /// Classify the batch from its counts alone.
    #[must_use]
    pub const fn work_class(self) -> WorkClass {
        if self.computed > self.requested {
            // More computed than requested cannot happen; believing such a record would let a
            // corrupt count launder a reuse row into a throughput row.
            return WorkClass::Impossible;
        }
        if self.requested == 0 {
            return WorkClass::NothingComputed;
        }
        if self.computed == 0 {
            return WorkClass::NothingComputed;
        }
        if self.computed == self.requested {
            WorkClass::FullComputation
        } else {
            WorkClass::Reuse {
                reused: self.requested - self.computed,
            }
        }
    }

    /// Whether a THROUGHPUT claim may be made from a timing over this batch.
    ///
    /// Deliberately narrow: only a full computation qualifies. A batch that reused even one document
    /// spent less work than its `requested` count suggests, and a rate computed from the requested
    /// count would overstate the engine by exactly the reuse.
    #[must_use]
    pub const fn admits_throughput_claim(self) -> bool {
        matches!(self.work_class(), WorkClass::FullComputation)
    }

    /// The fraction of the batch that was actually computed, for provenance on a reuse row.
    ///
    /// `None` for an empty or impossible batch rather than a silent 0.0 or 1.0, both of which would
    /// read as a meaningful measurement of nothing.
    #[must_use]
    pub fn computed_fraction(self) -> Option<f64> {
        if self.requested == 0 || self.computed > self.requested {
            return None;
        }
        // `allow`, not `expect`, matching font_metrics.rs in this crate: cast_precision_loss is a
        // pedantic lint, so if it is not enabled an `expect` goes UNFULFILLED — and an unfulfilled
        // expectation is itself a warning that CI turns into an error under -D warnings.
        // Batch sizes are far below 2^53 and the ratio is provenance, not arithmetic.
        #[allow(clippy::cast_precision_loss)]
        Some(self.computed as f64 / self.requested as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::{ReuseProof, WorkClass};

    /// The bd-bh7d artifact, expressed in counts.
    ///
    /// 342,682 iterations over a 25-document corpus where only the first pass computed anything.
    /// The clock said 16 ns; the counts say reuse. This is the case the type exists for.
    #[test]
    fn the_historical_artifact_classifies_as_reuse_not_throughput() {
        let proof = ReuseProof {
            requested: 342_682,
            computed: 25,
        };

        assert_eq!(proof.work_class(), WorkClass::Reuse { reused: 342_657 });
        assert!(
            !proof.admits_throughput_claim(),
            "a batch that computed 25 of 342,682 documents must not support a throughput claim"
        );
    }

    /// CONTROL: an honest full batch DOES admit a throughput claim.
    ///
    /// Without this the type could refuse everything, which would be safe and useless — every real
    /// measurement would be reclassified as reuse and no row could ever be banked.
    #[test]
    fn a_full_computation_admits_a_throughput_claim() {
        let proof = ReuseProof {
            requested: 512,
            computed: 512,
        };
        assert_eq!(proof.work_class(), WorkClass::FullComputation);
        assert!(proof.admits_throughput_claim());
        assert_eq!(proof.computed_fraction(), Some(1.0));
    }

    /// A single reused document is enough to disqualify a throughput claim.
    ///
    /// The boundary matters: a rate computed from `requested` would overstate the engine by exactly
    /// the reuse, and there is no threshold of "a little reuse" that is safe to ignore.
    #[test]
    fn one_reused_document_disqualifies_the_claim() {
        let proof = ReuseProof {
            requested: 512,
            computed: 511,
        };
        assert_eq!(proof.work_class(), WorkClass::Reuse { reused: 1 });
        assert!(!proof.admits_throughput_claim());
    }

    /// Nothing computed, and empty batches, are not measurements.
    #[test]
    fn empty_and_zero_work_batches_are_not_measurements() {
        for proof in [
            ReuseProof {
                requested: 100,
                computed: 0,
            },
            ReuseProof {
                requested: 0,
                computed: 0,
            },
        ] {
            assert_eq!(proof.work_class(), WorkClass::NothingComputed);
            assert!(!proof.admits_throughput_claim());
        }
        assert_eq!(
            ReuseProof {
                requested: 0,
                computed: 0
            }
            .computed_fraction(),
            None,
            "an empty batch has no meaningful computed fraction"
        );
    }

    /// Impossible counts are refused rather than interpreted.
    ///
    /// A record claiming more computed than requested is corrupt, and the dangerous reading is the
    /// generous one: treating it as a full computation would let a bad count launder a reuse row
    /// into a throughput row.
    #[test]
    fn impossible_counts_are_refused() {
        let proof = ReuseProof {
            requested: 10,
            computed: 11,
        };
        assert_eq!(proof.work_class(), WorkClass::Impossible);
        assert!(!proof.admits_throughput_claim());
        assert_eq!(proof.computed_fraction(), None);
    }
}
