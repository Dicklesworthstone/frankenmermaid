//! Delta-debugging input minimizer for frankenmermaid.
//!
//! Shrinks a failing Mermaid input to the smallest version that still
//! reproduces the target failure signature. Uses the ddmin algorithm
//! with line-level and character-level reduction passes.

use std::time::{Duration, Instant};

/// What kind of failure we're trying to preserve during minimization.
#[derive(Debug, Clone)]
pub enum FailureSignature {
    /// The parse/layout/render pipeline panics.
    Panic,
    /// The pipeline takes longer than the given duration.
    Timeout(Duration),
    /// The output contains or does not contain a specific string.
    OutputContains(String),
    /// The output does NOT contain a specific string.
    OutputMissing(String),
    /// Two runs produce different output (non-determinism).
    NonDeterministic,
    /// Any diagnostic with Error severity is emitted.
    AnyError,
}

/// Result of a minimization run.
#[derive(Debug, Clone)]
pub struct MinimizeResult {
    /// The minimized input that still reproduces the failure.
    pub minimized_input: String,
    /// Number of lines in the original input.
    pub original_lines: usize,
    /// Number of lines in the minimized input.
    pub minimized_lines: usize,
    /// Number of test iterations performed.
    pub iterations: usize,
    /// Total time spent minimizing.
    pub elapsed: Duration,
}

/// Test whether a given input reproduces the failure signature.
fn test_failure(input: &str, signature: &FailureSignature) -> bool {
    match signature {
        FailureSignature::Panic => std::panic::catch_unwind(|| {
            let _ = fm_parser::parse(input);
        })
        .is_err(),

        FailureSignature::Timeout(max_duration) => {
            let start = Instant::now();
            let _ = fm_parser::parse(input);
            start.elapsed() > *max_duration
        }

        FailureSignature::OutputContains(needle) => {
            let result = fm_parser::parse(input);
            let json = serde_json::to_string(&result.ir).unwrap_or_default();
            json.contains(needle)
        }

        FailureSignature::OutputMissing(needle) => {
            let result = fm_parser::parse(input);
            let json = serde_json::to_string(&result.ir).unwrap_or_default();
            !json.contains(needle)
        }

        FailureSignature::NonDeterministic => {
            let r1 = fm_parser::parse(input);
            let r2 = fm_parser::parse(input);
            let j1 = serde_json::to_string(&r1.ir).unwrap_or_default();
            let j2 = serde_json::to_string(&r2.ir).unwrap_or_default();
            j1 != j2
        }

        FailureSignature::AnyError => {
            let result = fm_parser::parse(input);
            result.ir.has_errors()
        }
    }
}

/// Minimize a failing input using delta debugging (ddmin).
///
/// The algorithm works in two passes:
/// 1. **Line-level**: Remove lines one at a time, keeping the failure.
/// 2. **Character-level**: For each remaining line, try trimming characters.
pub fn minimize(input: &str, signature: &FailureSignature) -> MinimizeResult {
    let start = Instant::now();
    let original_lines = input.lines().count();
    let mut iterations = 0_usize;

    // Verify the original input actually fails.
    if !test_failure(input, signature) {
        return MinimizeResult {
            minimized_input: input.to_string(),
            original_lines,
            minimized_lines: original_lines,
            iterations: 0,
            elapsed: start.elapsed(),
        };
    }

    // Pass 1: Line-level reduction.
    let mut lines: Vec<String> = input.lines().map(str::to_owned).collect();
    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < lines.len() {
            let mut candidate = lines.clone();
            candidate.remove(i);
            let candidate_input = candidate.join("\n");
            iterations += 1;

            if test_failure(&candidate_input, signature) {
                lines = candidate;
                changed = true;
                // Don't increment i — try removing the same position again.
            } else {
                i += 1;
            }

            // Safety limit.
            if iterations > 10_000 {
                break;
            }
        }
        if iterations > 10_000 {
            break;
        }
    }

    // Pass 2: Try removing contiguous blocks of 2, 4, 8 lines.
    for block_size in [8, 4, 2] {
        if lines.len() <= block_size {
            continue;
        }
        let mut i = 0;
        while i + block_size <= lines.len() {
            let mut candidate = lines.clone();
            candidate.drain(i..i + block_size);
            let candidate_input = candidate.join("\n");
            iterations += 1;

            if test_failure(&candidate_input, signature) {
                lines = candidate;
            } else {
                i += 1;
            }

            if iterations > 10_000 {
                break;
            }
        }
    }

    // Pass 3: Apply ddmin to each retained line.  Byte slicing would corrupt
    // non-ASCII labels, so candidates are built from `char_indices` boundaries.
    let mut line_index = 0;
    while line_index < lines.len() && iterations <= 10_000 {
        let mut granularity = 2;
        loop {
            let Some(line) = lines.get(line_index) else {
                break;
            };
            let boundaries: Vec<usize> = line
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(line.len()))
                .collect();
            let char_count = boundaries.len().saturating_sub(1);
            if char_count == 0 {
                break;
            }

            let chunk_size = char_count.div_ceil(granularity);
            let mut removed_chunk = false;
            let mut start = 0;
            while start < char_count {
                let end = (start + chunk_size).min(char_count);
                let (Some(prefix_end), Some(suffix_start)) =
                    (boundaries.get(start), boundaries.get(end))
                else {
                    break;
                };
                let (Some(prefix), Some(suffix)) =
                    (line.get(..*prefix_end), line.get(*suffix_start..))
                else {
                    break;
                };
                let mut candidate_line = String::with_capacity(line.len());
                candidate_line.push_str(prefix);
                candidate_line.push_str(suffix);

                let mut candidate = lines.clone();
                let Some(candidate_slot) = candidate.get_mut(line_index) else {
                    break;
                };
                *candidate_slot = candidate_line;
                let candidate_input = candidate.join("\n");
                iterations += 1;

                if test_failure(&candidate_input, signature) {
                    lines = candidate;
                    granularity = 2;
                    removed_chunk = true;
                    break;
                }

                if iterations > 10_000 {
                    break;
                }
                start = end;
            }

            if iterations > 10_000 || removed_chunk {
                if iterations > 10_000 {
                    break;
                }
                continue;
            }
            if granularity >= char_count {
                break;
            }
            granularity = (granularity * 2).min(char_count);
        }
        line_index += 1;
    }

    let minimized_input = lines.join("\n");
    let minimized_lines = lines.len();

    MinimizeResult {
        minimized_input,
        original_lines,
        minimized_lines,
        iterations,
        elapsed: start.elapsed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimizes_to_empty_when_always_fails() {
        let input = "flowchart LR\n  A --> B\n  B --> C\n  C --> D";
        // AnyError won't fire on valid input, so this should return unchanged.
        let result = minimize(input, &FailureSignature::AnyError);
        // Valid input doesn't have errors, so original is returned.
        assert_eq!(result.minimized_input, input);
        assert_eq!(result.iterations, 0);
    }

    #[test]
    fn minimize_preserves_output_contains() {
        let input = "flowchart LR\n  A --> B\n  B --> C\n  C --> D\n  D --> E";
        // The minimized version should still contain node "A".
        let result = minimize(
            input,
            &FailureSignature::OutputContains("\"A\"".to_string()),
        );
        assert!(result.minimized_input.contains('A'));
        assert!(result.minimized_lines <= result.original_lines);
    }

    #[test]
    fn minimize_result_tracks_iterations() {
        let input = "flowchart LR\n  A --> B";
        let result = minimize(
            input,
            &FailureSignature::OutputContains("\"A\"".to_string()),
        );
        // Should have done some iterations.
        assert!(result.iterations > 0);
    }

    #[test]
    fn minimizes_characters_without_splitting_utf8_labels() {
        let input = "flowchart LR\n  A[éclair] --> B";
        let signature = FailureSignature::OutputContains("\"A\"".to_string());

        let result = minimize(input, &signature);

        assert!(result.minimized_input.len() < input.len());
        assert!(result.minimized_input.contains('A'));
        assert!(test_failure(&result.minimized_input, &signature));
    }

    #[test]
    fn non_deterministic_on_deterministic_input_returns_unchanged() {
        let input = "flowchart LR\n  A --> B";
        let result = minimize(input, &FailureSignature::NonDeterministic);
        // Deterministic input should not trigger non-determinism.
        assert_eq!(result.minimized_input, input);
        assert_eq!(result.iterations, 0);
    }
}
