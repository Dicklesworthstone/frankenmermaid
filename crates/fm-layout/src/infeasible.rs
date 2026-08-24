//! Infeasibility diagnosis for the constraint solver (bd-1fef.2 item 4).
//!
//! When the LP is infeasible the solver falls back to the heuristic coordinates and says nothing
//! about WHY. That is safe but unhelpful: the author wrote constraints that cannot all hold, and the
//! only actionable answer is WHICH ONES conflict. "Infeasible" names the symptom; a minimal
//! conflicting set names the cause, and every constraint carries a `Span`, so the answer can point
//! at the lines the author wrote.
//!
//! # The algorithm, and why it is generic over the oracle
//!
//! This is the classic DELETION FILTER for an irreducible infeasible subset. Walk the constraints;
//! for each one, ask whether the set is STILL infeasible without it. If it is, that constraint was
//! not needed for the conflict and is dropped permanently. What survives is infeasible, and removing
//! any single member of it makes the rest feasible — which is what "irreducible" means.
//!
//! The feasibility oracle is a parameter rather than a direct call into `good_lp`. Two reasons, and
//! the second is the load-bearing one:
//!
//!   * the filter is pure combinatorics and deserves tests that cannot be broken by a solver
//!     upgrade, a time limit, or a missing backend;
//!   * an LP oracle is EXPENSIVE — one solve per constraint — so the cost model matters, and a
//!     caller that wants to bound it needs to see the calls. Hiding the solver inside would hide
//!     the cost too.
//!
//! # Cost
//!
//! Exactly `n` oracle calls for `n` constraints, one per deletion attempt. That is the price of a
//! guaranteed-irreducible answer; cheaper heuristics exist but return a superset, which for a
//! diagnostic means telling the author to look at constraints that are not at fault.

use fm_core::IrConstraint;

/// A minimal set of mutually conflicting constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfeasibilityReport {
    /// Indices into the original constraint slice, ascending.
    ///
    /// Indices rather than clones so a caller can map back to whatever it holds — the IR, a source
    /// map, a diagnostic list — without this module knowing about any of them.
    pub conflicting: Vec<usize>,
    /// Oracle calls spent. Recorded because the oracle is an LP solve, and a diagnosis that
    /// silently costs `n` solves on a large diagram is a performance defect waiting to be found by
    /// a user rather than by us.
    pub oracle_calls: usize,
}

/// Find an irreducible infeasible subset, or `None` if the full set is already feasible.
///
/// `is_infeasible` receives the indices still under consideration, ascending. It must be a pure
/// function of that set: the filter relies on asking the same question twice and getting the same
/// answer, and a solver with a wall-clock time limit can violate that — which is why the caller
/// supplies the oracle and owns that decision.
pub fn diagnose<F>(constraint_count: usize, mut is_infeasible: F) -> Option<InfeasibilityReport>
where
    F: FnMut(&[usize]) -> bool,
{
    let mut oracle_calls = 0_usize;

    let mut candidate: Vec<usize> = (0..constraint_count).collect();

    // A feasible problem has no conflicting subset, and answering with one would be worse than
    // answering with nothing: it would send the author to edit constraints that are fine.
    oracle_calls += 1;
    if !is_infeasible(&candidate) {
        return None;
    }

    // Walk the ORIGINAL order, not the shrinking candidate, so the traversal is deterministic and
    // does not depend on how much has already been removed.
    for index in 0..constraint_count {
        if !candidate.contains(&index) {
            continue;
        }

        let trial: Vec<usize> = candidate.iter().copied().filter(|&i| i != index).collect();

        // An empty trial cannot be infeasible in any useful sense; the last constraint standing is
        // kept so the report is never empty while claiming a conflict exists.
        if trial.is_empty() {
            continue;
        }

        oracle_calls += 1;
        if is_infeasible(&trial) {
            candidate = trial;
        }
    }

    Some(InfeasibilityReport {
        conflicting: candidate,
        oracle_calls,
    })
}

/// Render a report against the constraints it indexes, for a user-facing diagnostic.
///
/// Names the constraint KIND and the nodes involved, because "constraint 3 conflicts with
/// constraint 7" is not actionable — the author wrote node names, not indices.
#[must_use]
pub fn describe(report: &InfeasibilityReport, constraints: &[IrConstraint]) -> String {
    let mut out = String::from("these constraints cannot all hold at once:");
    for &index in &report.conflicting {
        let Some(constraint) = constraints.get(index) else {
            continue;
        };
        out.push_str("\n  - ");
        match constraint {
            IrConstraint::SameRank { node_ids, .. } => {
                out.push_str(&format!("same rank for {}", node_ids.join(", ")));
            }
            IrConstraint::MinLength {
                from_id,
                to_id,
                min_len,
                ..
            } => {
                out.push_str(&format!(
                    "minimum length {min_len} from {from_id} to {to_id}"
                ));
            }
            IrConstraint::Pin { node_id, x, y, .. } => {
                out.push_str(&format!("{node_id} pinned to ({x}, {y})"));
            }
            IrConstraint::OrderInRank { node_ids, .. } => {
                out.push_str(&format!("order within rank: {}", node_ids.join(" before ")));
            }
            IrConstraint::NonOverlap { node_ids, gap, .. } => {
                let scope = if node_ids.is_empty() {
                    "all nodes".to_string()
                } else {
                    node_ids.join(", ")
                };
                out.push_str(&format!("non-overlap for {scope} with gap {gap}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{describe, diagnose};
    use fm_core::{IrConstraint, Span};

    /// An oracle whose conflict is a known pair: infeasible exactly when BOTH are present.
    fn conflicting_pair(a: usize, b: usize) -> impl FnMut(&[usize]) -> bool {
        move |set: &[usize]| set.contains(&a) && set.contains(&b)
    }

    /// The filter must return exactly the conflicting pair, not the whole set.
    ///
    /// The point of the feature: a diagnosis that named all five constraints would be true and
    /// useless, since the author would have to find the real pair themselves.
    #[test]
    fn the_filter_narrows_to_the_conflicting_pair() {
        let report = diagnose(5, conflicting_pair(1, 3)).expect("the set is infeasible");
        assert_eq!(
            report.conflicting,
            vec![1, 3],
            "expected only the conflicting pair"
        );
    }

    /// CONTROL: a feasible set yields no report at all.
    ///
    /// Without this, an implementation that always returned SOMETHING would pass the test above and
    /// send authors to edit constraints that are not at fault.
    #[test]
    fn a_feasible_set_is_not_diagnosed() {
        assert!(diagnose(4, |_| false).is_none());
        // One oracle call, and no filtering work: proven by the absence of a report rather than by
        // inspecting internals.
        assert!(diagnose(0, |_| false).is_none());
    }

    /// The result must be IRREDUCIBLE: removing any member makes the rest feasible.
    ///
    /// Checked against the oracle itself rather than against an expected list, so the property is
    /// verified rather than restated.
    #[test]
    fn the_result_is_irreducible() {
        let mut oracle = conflicting_pair(2, 6);
        let report = diagnose(8, &mut oracle).expect("infeasible");

        assert!(
            oracle(&report.conflicting),
            "the reported set must itself be infeasible"
        );
        for &member in &report.conflicting {
            let without: Vec<usize> = report
                .conflicting
                .iter()
                .copied()
                .filter(|&i| i != member)
                .collect();
            assert!(
                !oracle(&without),
                "removing {member} left the set infeasible, so it was not irreducible"
            );
        }
    }

    /// The oracle is called once per constraint plus the initial check, and the count is reported.
    ///
    /// Pinned because the oracle is an LP SOLVE: a change that made this quadratic would not fail
    /// any correctness test, it would just make a large diagram hang.
    #[test]
    fn the_oracle_call_count_is_linear_and_reported() {
        let report = diagnose(10, conflicting_pair(0, 9)).expect("infeasible");
        assert_eq!(
            report.oracle_calls, 11,
            "expected one initial check plus one per constraint"
        );
    }

    /// A single-constraint conflict is reported rather than filtered away to nothing.
    #[test]
    fn a_lone_infeasible_constraint_is_still_reported() {
        // Infeasible whenever constraint 0 is present, including on its own.
        let report = diagnose(3, |set: &[usize]| set.contains(&0)).expect("infeasible");
        assert_eq!(report.conflicting, vec![0]);
    }

    /// The description names nodes and kinds, because indices are not actionable.
    #[test]
    fn the_description_names_the_constraints_not_their_indices() {
        let constraints = vec![
            IrConstraint::SameRank {
                node_ids: vec![String::from("a"), String::from("b")],
                span: Span::default(),
            },
            IrConstraint::Pin {
                node_id: String::from("a"),
                x: 10.0,
                y: 20.0,
                span: Span::default(),
            },
        ];
        let report = diagnose(2, conflicting_pair(0, 1)).expect("infeasible");
        let text = describe(&report, &constraints);

        assert!(
            text.contains("same rank for a, b"),
            "kinds must be named: {text}"
        );
        assert!(
            text.contains("a pinned to (10, 20)"),
            "nodes must be named: {text}"
        );
        assert!(
            !text.contains("constraint 0"),
            "indices are not actionable: {text}"
        );
    }
}
