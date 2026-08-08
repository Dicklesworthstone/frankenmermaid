//! Delta-debugging input minimizer for frankenmermaid.
//!
//! Shrinks a failing Mermaid input to the smallest version that still
//! reproduces the target failure signature. Uses the ddmin algorithm
//! with line-level and character-level reduction passes.

use std::time::{Duration, Instant};

use serde::Serialize;

/// Which pipeline stage the failure probe runs before evaluating the signature.
///
/// A parser bug reproduces at [`Stage::Parse`], but a layout or render bug needs the pipeline
/// carried further — reducing a render defect against parse output shrinks the input until the
/// defect is gone while the probe still reports "reproduced".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// Parse only; the probe output is the diagram IR as JSON.
    Parse,
    /// Parse then lay out; the probe output is the layout as JSON.
    Layout,
    /// Parse, lay out, then render; the probe output is the SVG document.
    Render,
}

impl Stage {
    /// Stable identifier used in reduction reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::Layout => "layout",
            Self::Render => "render",
        }
    }
}

/// What kind of failure we're trying to preserve during minimization.
#[derive(Debug, Clone)]
pub enum FailureSignature {
    /// The pipeline panics.
    Panic,
    /// The pipeline takes longer than the given duration.
    Timeout(Duration),
    /// The stage output contains a specific string.
    OutputContains(String),
    /// The stage output does NOT contain a specific string.
    OutputMissing(String),
    /// Two runs produce different output (non-determinism).
    NonDeterministic,
    /// Any diagnostic with Error severity is emitted. Always evaluated on parse diagnostics,
    /// because that is the only stage that produces them.
    AnyError,
}

impl FailureSignature {
    /// Stable identifier used in reduction reports.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::Timeout(_) => "timeout",
            Self::OutputContains(_) => "output-contains",
            Self::OutputMissing(_) => "output-missing",
            Self::NonDeterministic => "non-deterministic",
            Self::AnyError => "any-error",
        }
    }
}

/// Knobs for a reduction run.
#[derive(Debug, Clone, Copy)]
pub struct MinimizeOptions {
    /// Pipeline stage the probe exercises.
    pub stage: Stage,
    /// Upper bound on probe iterations. Reaching it truncates the reduction and is reported.
    pub max_iterations: usize,
}

impl Default for MinimizeOptions {
    fn default() -> Self {
        Self {
            stage: Stage::Parse,
            max_iterations: 10_000,
        }
    }
}

/// Result of a minimization run.
#[derive(Debug, Clone, Serialize)]
pub struct MinimizeResult {
    /// The minimized input that still reproduces the failure.
    pub minimized_input: String,
    /// Whether the ORIGINAL input reproduced the signature at all. When false, no reduction was
    /// attempted and `minimized_input` is the untouched original — the signature, the stage, or
    /// the input is wrong, and the caller must not read the result as "already minimal".
    pub reproduced: bool,
    /// Pipeline stage the probe exercised.
    pub stage: Stage,
    /// Signature identifier the reduction preserved.
    pub signature: &'static str,
    /// Number of lines in the original input.
    pub original_lines: usize,
    /// Number of lines in the minimized input.
    pub minimized_lines: usize,
    /// Number of bytes in the original input.
    pub original_bytes: usize,
    /// Number of bytes in the minimized input.
    pub minimized_bytes: usize,
    /// Number of test iterations performed.
    pub iterations: usize,
    /// Whether the iteration budget was exhausted, meaning the result may still be reducible.
    pub hit_iteration_cap: bool,
    /// Total time spent minimizing.
    pub elapsed: Duration,
}

/// Whether [`FailureSignature::Panic`] can be observed in this build.
///
/// `catch_unwind` cannot intercept a panic when the profile sets `panic = "abort"`, which the
/// release profile does. Callers must check this instead of silently reducing against a probe
/// that can never report a panic.
#[must_use]
pub const fn panic_capture_available() -> bool {
    !cfg!(panic = "abort")
}

/// Run the pipeline through `stage` and return the stage's output as text.
fn probe_output(input: &str, stage: Stage) -> String {
    let parsed = fm_parser::parse(input);
    match stage {
        Stage::Parse => serde_json::to_string(&parsed.ir).unwrap_or_default(),
        // Reuses the determinism manifest's canonical encoding rather than a second one, so a
        // reduction sees exactly the coordinates the manifest digests.
        Stage::Layout => crate::canonical_layout(&fm_layout::layout_diagram(&parsed.ir)),
        Stage::Render => fm_render_svg::render_svg(&parsed.ir),
    }
}

/// Test whether a given input reproduces the failure signature.
fn test_failure(input: &str, signature: &FailureSignature, stage: Stage) -> bool {
    match signature {
        FailureSignature::Panic => std::panic::catch_unwind(|| {
            let _ = probe_output(input, stage);
        })
        .is_err(),

        FailureSignature::Timeout(max_duration) => {
            let start = Instant::now();
            let output = probe_output(input, stage);
            let elapsed = start.elapsed();
            // Keep the work observable so the stage cannot be optimized away between the clock
            // reads, which would make every candidate look instant.
            std::hint::black_box(output.len());
            elapsed > *max_duration
        }

        FailureSignature::OutputContains(needle) => probe_output(input, stage).contains(needle),

        FailureSignature::OutputMissing(needle) => !probe_output(input, stage).contains(needle),

        FailureSignature::NonDeterministic => {
            probe_output(input, stage) != probe_output(input, stage)
        }

        // Diagnostics exist only at parse time, so this signature ignores `stage` by design.
        FailureSignature::AnyError => fm_parser::parse(input).ir.has_errors(),
    }
}

/// Minimize a failing input using delta debugging (ddmin).
///
/// The algorithm works in three passes:
/// 1. **Line-level**: Remove lines one at a time, keeping the failure.
/// 2. **Block-level**: Remove contiguous blocks of 8, 4, then 2 lines.
/// 3. **Character-level**: For each remaining line, try trimming chunks of characters.
pub fn minimize(
    input: &str,
    signature: &FailureSignature,
    options: MinimizeOptions,
) -> MinimizeResult {
    let start = Instant::now();
    let original_lines = input.lines().count();
    let max_iterations = options.max_iterations;
    let stage = options.stage;
    let mut iterations = 0_usize;

    // Verify the original input actually fails.
    if !test_failure(input, signature, stage) {
        return MinimizeResult {
            minimized_input: input.to_string(),
            reproduced: false,
            stage,
            signature: signature.as_str(),
            original_lines,
            minimized_lines: original_lines,
            original_bytes: input.len(),
            minimized_bytes: input.len(),
            iterations: 0,
            hit_iteration_cap: false,
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

            if test_failure(&candidate_input, signature, stage) {
                lines = candidate;
                changed = true;
                // Don't increment i — try removing the same position again.
            } else {
                i += 1;
            }

            // Safety limit.
            if iterations > max_iterations {
                break;
            }
        }
        if iterations > max_iterations {
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

            if test_failure(&candidate_input, signature, stage) {
                lines = candidate;
            } else {
                i += 1;
            }

            if iterations > max_iterations {
                break;
            }
        }
    }

    // Pass 3: Apply ddmin to each retained line.  Byte slicing would corrupt
    // non-ASCII labels, so candidates are built from `char_indices` boundaries.
    let mut line_index = 0;
    while line_index < lines.len() && iterations <= max_iterations {
        let mut granularity = 2;
        // The line is cloned out because the body reassigns `lines` whenever a chunk removal
        // reproduces, which a borrow held across the loop condition would forbid.
        while let Some(line) = lines.get(line_index).cloned() {
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
            let mut start_char = 0;
            while start_char < char_count {
                let end = (start_char + chunk_size).min(char_count);
                let (Some(prefix_end), Some(suffix_start)) =
                    (boundaries.get(start_char), boundaries.get(end))
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

                if test_failure(&candidate_input, signature, stage) {
                    lines = candidate;
                    granularity = 2;
                    removed_chunk = true;
                    break;
                }

                if iterations > max_iterations {
                    break;
                }
                start_char = end;
            }

            if iterations > max_iterations || removed_chunk {
                if iterations > max_iterations {
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
        reproduced: true,
        stage,
        signature: signature.as_str(),
        original_lines,
        minimized_lines,
        original_bytes: input.len(),
        minimized_bytes: minimized_input.len(),
        minimized_input,
        iterations,
        hit_iteration_cap: iterations > max_iterations,
        elapsed: start.elapsed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_options() -> MinimizeOptions {
        MinimizeOptions::default()
    }

    #[test]
    fn minimizes_to_empty_when_always_fails() {
        let input = "flowchart LR\n  A --> B\n  B --> C\n  C --> D";
        // AnyError won't fire on valid input, so this should return unchanged.
        let result = minimize(input, &FailureSignature::AnyError, parse_options());
        // Valid input doesn't have errors, so original is returned.
        assert_eq!(result.minimized_input, input);
        assert_eq!(result.iterations, 0);
        assert!(
            !result.reproduced,
            "a signature that never fired must be reported as not reproduced"
        );
    }

    #[test]
    fn minimize_preserves_output_contains() {
        let input = "flowchart LR\n  A --> B\n  B --> C\n  C --> D\n  D --> E";
        // The minimized version should still contain node "A".
        let result = minimize(
            input,
            &FailureSignature::OutputContains("\"A\"".to_string()),
            parse_options(),
        );
        assert!(result.reproduced);
        assert!(result.minimized_input.contains('A'));
        assert!(result.minimized_lines <= result.original_lines);
        assert!(result.minimized_bytes <= result.original_bytes);
    }

    #[test]
    fn minimize_result_tracks_iterations() {
        let input = "flowchart LR\n  A --> B";
        let result = minimize(
            input,
            &FailureSignature::OutputContains("\"A\"".to_string()),
            parse_options(),
        );
        // Should have done some iterations.
        assert!(result.iterations > 0);
    }

    #[test]
    fn minimizes_characters_without_splitting_utf8_labels() {
        let input = "flowchart LR\n  A[éclair] --> B";
        let signature = FailureSignature::OutputContains("\"A\"".to_string());

        let result = minimize(input, &signature, parse_options());

        assert!(result.minimized_input.len() < input.len());
        assert!(result.minimized_input.contains('A'));
        assert!(test_failure(
            &result.minimized_input,
            &signature,
            Stage::Parse
        ));
    }

    #[test]
    fn non_deterministic_on_deterministic_input_returns_unchanged() {
        let input = "flowchart LR\n  A --> B";
        let result = minimize(input, &FailureSignature::NonDeterministic, parse_options());
        // Deterministic input should not trigger non-determinism.
        assert_eq!(result.minimized_input, input);
        assert_eq!(result.iterations, 0);
        assert!(!result.reproduced);
    }

    #[test]
    fn render_stage_reduction_keeps_a_marker_only_the_renderer_emits() {
        // The `fm-edge-dashed` class is emitted only for a dotted edge and only by the renderer,
        // so this signature can be preserved only by a probe that carries the pipeline that far.
        // A bare `stroke-dasharray` needle would NOT work: the theme CSS mentions it
        // unconditionally, so every candidate down to the empty input would "reproduce".
        let input = concat!(
            "flowchart LR\n",
            "  A --> B\n",
            "  B -.-> C\n",
            "  C --> D\n",
            "  D --> E\n"
        );
        let signature = FailureSignature::OutputContains("fm-edge-dashed".to_string());

        let parse_probe = minimize(input, &signature, parse_options());
        assert!(
            !parse_probe.reproduced,
            "parse output must not contain a render-only marker, else this test proves nothing"
        );

        let result = minimize(
            input,
            &signature,
            MinimizeOptions {
                stage: Stage::Render,
                ..MinimizeOptions::default()
            },
        );

        assert!(result.reproduced);
        assert!(result.minimized_lines < result.original_lines);
        // The dotted-edge token must survive because it is the cause. The character pass reduces
        // past the well-formed `-.->` arrow: a bare `B -.` still parses into a dotted edge under
        // recovery, so asserting on the full arrow would fight a correct, smaller reduction.
        assert!(
            result.minimized_input.contains("-."),
            "the dotted-edge token is the only source of the marker, so it must survive: {:?}",
            result.minimized_input
        );
        assert!(test_failure(
            &result.minimized_input,
            &signature,
            Stage::Render
        ));
    }

    #[test]
    fn layout_stage_reduction_keeps_geometry_only_layout_produces() {
        // Layout output carries positioned node bounds; parse output has no coordinates, so a
        // geometry needle separates the two stages.
        let input = "flowchart LR\n  A --> B\n  B --> C\n  C --> D";
        let signature = FailureSignature::OutputContains("bounds: x=".to_string());

        assert!(
            !minimize(input, &signature, parse_options()).reproduced,
            "parse output must not carry layout bounds, else this test proves nothing"
        );

        let result = minimize(
            input,
            &signature,
            MinimizeOptions {
                stage: Stage::Layout,
                ..MinimizeOptions::default()
            },
        );
        assert!(result.reproduced);
        assert!(test_failure(
            &result.minimized_input,
            &signature,
            Stage::Layout
        ));
    }

    #[test]
    fn iteration_budget_truncates_and_is_reported() {
        let input = "flowchart LR\n  A --> B\n  B --> C\n  C --> D\n  D --> E\n  E --> F";
        let result = minimize(
            input,
            &FailureSignature::OutputContains("\"A\"".to_string()),
            MinimizeOptions {
                stage: Stage::Parse,
                max_iterations: 1,
            },
        );
        assert!(result.reproduced);
        assert!(
            result.hit_iteration_cap,
            "a one-iteration budget on a six-line input must report truncation"
        );
    }

    #[test]
    fn panic_capture_availability_matches_the_unwind_strategy() {
        // Test profiles unwind, so the panic signature is usable here. The release profile sets
        // `panic = "abort"`, where the CLI must refuse rather than reduce against a blind probe.
        assert_eq!(panic_capture_available(), !cfg!(panic = "abort"));
    }
}
