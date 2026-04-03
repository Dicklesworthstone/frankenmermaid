//! Hierarchical delta debugging for test case minimization.
//!
//! Automatically reduces a failing diagram input to the smallest reproducer that
//! still triggers the failure. Uses Zeller & Hildebrandt's ddmin algorithm with
//! hierarchical levels: subgraphs → nodes → edges → properties.
//!
//! # Algorithm
//!
//! Delta debugging (ddmin) works by binary search over subsets:
//!
//! 1. Split the input into n chunks.
//! 2. Test each chunk's complement (input minus that chunk).
//! 3. If removing a chunk still fails, keep the reduced input and recurse.
//! 4. If no single chunk removal works, increase granularity (n *= 2) and retry.
//! 5. Stop when n >= |input| (1-minimal).
//!
//! The hierarchical variant applies ddmin at multiple levels of the IR structure:
//! first try removing entire subgraphs, then individual nodes, then edges,
//! then style/class definitions.
//!
//! # References
//!
//! - Zeller & Hildebrandt, "Simplifying and Isolating Failure-Inducing Input" (TSE 2002)
//! - Misherghi & Su, "HDD: Hierarchical Delta Debugging" (ICSE 2006)

use tracing::debug;

/// Result of running the test function on a reduced input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestOutcome {
    /// The test still fails (the bug is still present in the reduced input).
    Fail,
    /// The test passes (the removed element was necessary for the bug).
    Pass,
    /// The test is inconclusive (e.g., the input is invalid after reduction).
    Unresolved,
}

/// Configuration for delta debugging.
#[derive(Debug, Clone, Copy)]
pub struct DeltaDebugConfig {
    /// Maximum number of ddmin iterations before giving up. Default: 100.
    pub max_iterations: usize,
    /// Whether to apply hierarchical reduction (subgraphs → nodes → edges).
    pub hierarchical: bool,
}

impl Default for DeltaDebugConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            hierarchical: true,
        }
    }
}

/// Result of delta debugging minimization.
#[derive(Debug, Clone)]
pub struct DeltaDebugResult<T> {
    /// The minimized input.
    pub minimized: Vec<T>,
    /// Number of test invocations used.
    pub test_calls: usize,
    /// Number of elements removed.
    pub elements_removed: usize,
    /// Original input size.
    pub original_size: usize,
}

/// Run the ddmin algorithm to find a 1-minimal failing subset.
///
/// Given a set of elements and a test function, find the smallest subset
/// that still causes the test to return `TestOutcome::Fail`.
///
/// # Arguments
/// * `elements` - The input elements to minimize.
/// * `test_fn` - A function that tests a subset and returns the outcome.
/// * `config` - Configuration for the algorithm.
///
/// # Returns
/// A `DeltaDebugResult` with the minimal failing subset.
pub fn ddmin<T: Clone>(
    elements: &[T],
    test_fn: &dyn Fn(&[T]) -> TestOutcome,
    config: &DeltaDebugConfig,
) -> DeltaDebugResult<T> {
    let original_size = elements.len();

    if original_size <= 1 {
        return DeltaDebugResult {
            minimized: elements.to_vec(),
            test_calls: 0,
            elements_removed: 0,
            original_size,
        };
    }

    let mut current = elements.to_vec();
    let mut n = 2_usize;
    let mut test_calls = 0_usize;
    let mut iterations = 0_usize;

    while current.len() >= 2 && iterations < config.max_iterations {
        iterations += 1;
        let chunk_size = current.len().div_ceil(n);
        let mut reduced = false;

        // Try removing each chunk.
        let num_chunks = n;
        let mut chunk_idx = 0;
        while chunk_idx < num_chunks {
            let start = chunk_idx * chunk_size;
            let end = ((chunk_idx + 1) * chunk_size).min(current.len());
            if start >= current.len() {
                break;
            }

            // Build complement: everything except this chunk.
            let complement: Vec<T> = current[..start]
                .iter()
                .chain(current[end..].iter())
                .cloned()
                .collect();

            if complement.is_empty() {
                chunk_idx += 1;
                continue;
            }

            test_calls += 1;
            let outcome = test_fn(&complement);

            if outcome == TestOutcome::Fail {
                // This chunk wasn't needed — keep the complement.
                current = complement;
                n = (n - 1).max(2);
                reduced = true;

                debug!(
                    remaining = current.len(),
                    removed_chunk_start = start,
                    removed_chunk_end = end,
                    "ddmin: removed chunk, still fails"
                );
                break;
            }
            chunk_idx += 1;
        }

        if !reduced {
            if n >= current.len() {
                break; // 1-minimal
            }
            n = (n * 2).min(current.len());
        }
    }

    let elements_removed = original_size - current.len();

    debug!(
        original_size,
        minimized_size = current.len(),
        elements_removed,
        test_calls,
        "ddmin complete"
    );

    DeltaDebugResult {
        minimized: current,
        test_calls,
        elements_removed,
        original_size,
    }
}

/// Hierarchical delta debugging for diagram IR elements.
///
/// Applies ddmin at multiple levels:
/// 1. First try removing groups of elements (coarse grain).
/// 2. Then try removing individual elements (fine grain).
///
/// The `group_fn` assigns each element to a group (e.g., by subgraph or rank).
/// Elements in the same group are removed together in the coarse pass.
pub fn hierarchical_ddmin<T: Clone>(
    elements: &[T],
    test_fn: &dyn Fn(&[T]) -> TestOutcome,
    group_fn: &dyn Fn(&T) -> usize,
    config: &DeltaDebugConfig,
) -> DeltaDebugResult<T> {
    let original_size = elements.len();

    if !config.hierarchical {
        return ddmin(elements, test_fn, config);
    }

    // Phase 1: Coarse-grain reduction by groups.
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, elem) in elements.iter().enumerate() {
        groups.entry(group_fn(elem)).or_default().push(i);
    }

    let group_ids: Vec<usize> = groups.keys().copied().collect();
    let mut total_test_calls = 0;

    // Try removing each group.
    let group_result = ddmin(
        &group_ids,
        &|subset: &[usize]| {
            let subset_set: std::collections::BTreeSet<usize> = subset.iter().copied().collect();
            let reduced: Vec<T> = elements
                .iter()
                .filter(|elem| subset_set.contains(&group_fn(elem)))
                .cloned()
                .collect();
            if reduced.is_empty() {
                TestOutcome::Unresolved
            } else {
                test_fn(&reduced)
            }
        },
        config,
    );
    let kept_groups = group_result.minimized;
    total_test_calls += group_result.test_calls;

    // Build reduced set from kept groups.
    let kept_set: std::collections::BTreeSet<usize> = kept_groups.iter().copied().collect();
    let coarse_reduced: Vec<T> = elements
        .iter()
        .enumerate()
        .filter(|(_, elem)| kept_set.contains(&group_fn(elem)))
        .map(|(_, elem)| elem.clone())
        .collect();

    debug!(
        coarse_size = coarse_reduced.len(),
        groups_removed = group_ids.len() - kept_groups.len(),
        "Hierarchical ddmin: coarse phase complete"
    );

    // Phase 2: Fine-grain reduction on the coarse result.
    let fine_result = ddmin(&coarse_reduced, test_fn, config);
    total_test_calls += fine_result.test_calls;

    DeltaDebugResult {
        minimized: fine_result.minimized.clone(),
        test_calls: total_test_calls,
        elements_removed: original_size - fine_result.minimized.len(),
        original_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddmin_empty_input() {
        let result = ddmin::<i32>(&[], &|_| TestOutcome::Fail, &DeltaDebugConfig::default());
        assert!(result.minimized.is_empty());
        assert_eq!(result.test_calls, 0);
    }

    #[test]
    fn ddmin_single_element() {
        let result = ddmin(&[42], &|_| TestOutcome::Fail, &DeltaDebugConfig::default());
        assert_eq!(result.minimized, vec![42]);
        assert_eq!(result.test_calls, 0);
    }

    #[test]
    fn ddmin_finds_single_culprit() {
        // Elements 0..10, but only element 7 causes failure.
        let elements: Vec<i32> = (0..10).collect();
        let result = ddmin(
            &elements,
            &|subset| {
                if subset.contains(&7) {
                    TestOutcome::Fail
                } else {
                    TestOutcome::Pass
                }
            },
            &DeltaDebugConfig::default(),
        );

        assert_eq!(result.minimized, vec![7]);
        assert_eq!(result.original_size, 10);
        assert_eq!(result.elements_removed, 9);
    }

    #[test]
    fn ddmin_finds_pair_culprit() {
        // Elements 0..10, but elements 3 AND 7 together cause failure.
        let elements: Vec<i32> = (0..10).collect();
        let result = ddmin(
            &elements,
            &|subset| {
                if subset.contains(&3) && subset.contains(&7) {
                    TestOutcome::Fail
                } else {
                    TestOutcome::Pass
                }
            },
            &DeltaDebugConfig::default(),
        );

        assert!(result.minimized.contains(&3));
        assert!(result.minimized.contains(&7));
        assert!(
            result.minimized.len() <= 4,
            "Should minimize to near-minimal, got {} elements",
            result.minimized.len()
        );
    }

    #[test]
    fn ddmin_all_elements_needed() {
        // All elements are needed for failure.
        let elements = vec![1, 2, 3];
        let result = ddmin(
            &elements,
            &|subset| {
                if subset.len() == 3 {
                    TestOutcome::Fail
                } else {
                    TestOutcome::Pass
                }
            },
            &DeltaDebugConfig::default(),
        );

        assert_eq!(result.minimized.len(), 3);
        assert_eq!(result.elements_removed, 0);
    }

    #[test]
    fn ddmin_respects_max_iterations() {
        let elements: Vec<i32> = (0..100).collect();
        let config = DeltaDebugConfig {
            max_iterations: 5,
            ..Default::default()
        };
        let result = ddmin(&elements, &|_| TestOutcome::Fail, &config);
        // Should terminate within max_iterations even if always failing.
        assert!(result.test_calls <= 100);
    }

    #[test]
    fn ddmin_handles_unresolved() {
        // Even-sized subsets are unresolved, odd-sized subsets with 5 fail.
        let elements: Vec<i32> = (0..8).collect();
        let result = ddmin(
            &elements,
            &|subset| {
                if subset.len() % 2 == 0 && subset.contains(&5) {
                    TestOutcome::Fail
                } else if subset.len() % 2 != 0 {
                    TestOutcome::Unresolved
                } else {
                    TestOutcome::Pass
                }
            },
            &DeltaDebugConfig::default(),
        );

        // Should find some minimal subset containing 5.
        assert!(result.minimized.contains(&5));
    }

    #[test]
    fn ddmin_large_input_single_culprit() {
        let elements: Vec<i32> = (0..1000).collect();
        let result = ddmin(
            &elements,
            &|subset| {
                if subset.contains(&573) {
                    TestOutcome::Fail
                } else {
                    TestOutcome::Pass
                }
            },
            &DeltaDebugConfig::default(),
        );

        assert_eq!(result.minimized, vec![573]);
        // ddmin should be efficient: O(log n) for single-element bugs.
        assert!(
            result.test_calls < 50,
            "Too many test calls: {} for 1000-element single culprit",
            result.test_calls
        );
    }

    #[test]
    fn hierarchical_ddmin_groups() {
        // 12 elements in 3 groups. Group 1 (elements 4-7) causes failure.
        let elements: Vec<i32> = (0..12).collect();
        let result = hierarchical_ddmin(
            &elements,
            &|subset| {
                if subset.iter().any(|&x| (4..8).contains(&x)) {
                    TestOutcome::Fail
                } else {
                    TestOutcome::Pass
                }
            },
            &|&elem| (elem / 4) as usize, // Groups: 0-3, 4-7, 8-11
            &DeltaDebugConfig::default(),
        );

        // Should find a minimal subset from group 1.
        assert!(result.minimized.iter().all(|&x| (4..8).contains(&x)));
        assert!(
            result.minimized.len() <= 4,
            "Should minimize within group 1"
        );
    }

    #[test]
    fn hierarchical_disabled_falls_back_to_ddmin() {
        let elements: Vec<i32> = (0..10).collect();
        let config = DeltaDebugConfig {
            hierarchical: false,
            ..Default::default()
        };
        let result = hierarchical_ddmin(
            &elements,
            &|subset| {
                if subset.contains(&5) {
                    TestOutcome::Fail
                } else {
                    TestOutcome::Pass
                }
            },
            &|_| 0,
            &config,
        );

        assert_eq!(result.minimized, vec![5]);
    }

    #[test]
    fn ddmin_deterministic() {
        let elements: Vec<i32> = (0..20).collect();
        let test = |subset: &[i32]| -> TestOutcome {
            if subset.contains(&3) && subset.contains(&15) {
                TestOutcome::Fail
            } else {
                TestOutcome::Pass
            }
        };
        let config = DeltaDebugConfig::default();

        let r1 = ddmin(&elements, &test, &config);
        let r2 = ddmin(&elements, &test, &config);

        assert_eq!(r1.minimized, r2.minimized);
        assert_eq!(r1.test_calls, r2.test_calls);
    }
}
