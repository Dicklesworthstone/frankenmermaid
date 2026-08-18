//! Terminal output determinism, where a one-ULP difference becomes a whole cell (bd-1s1g.6).
//!
//! The other suites in this bead check geometry (fm-layout), solver coordinates (fm-cli),
//! serialised SVG (fm-render-svg) and the canvas op stream (fm-render-canvas). The terminal is the
//! remaining renderer and it has a property none of them do:
//!
//! **It QUANTISES.** Every float is floored or ceiled into an integer cell, so a floating-point
//! difference has exactly two possible fates. Almost always it vanishes -- the value was nowhere
//! near a boundary and the same cell wins. Occasionally it lands astride one, and then the smallest
//! representable numeric difference moves a glyph a FULL CELL.
//!
//! That makes the terminal the renderer where a divergence is least likely and most visible. It
//! also makes byte comparison the right instrument: the output is text, so an assertion can compare
//! exactly what a user would diff, with no tolerance to choose and nothing to round.
//!
//! These do not attempt to find a value sitting on a boundary. Manufacturing one requires knowing
//! the layout's arithmetic well enough to aim at it, and a test that aimed and missed would look
//! like a pass. They check the properties that hold regardless: same input, same bytes.

use fm_render_term::render_term;

const DIAGRAMS: &[(&str, &str)] = &[
    (
        "flowchart",
        "flowchart TD\n  a[Alpha] --> b[Beta]\n  b --> c[Gamma]\n  c -.-> a\n",
    ),
    (
        "sequence",
        "sequenceDiagram\n  participant A\n  participant B\n  A->>B: hello\n  B-->>A: reply\n",
    ),
    (
        "class",
        "classDiagram\n  class Alpha {\n    +String name\n  }\n  Alpha <|-- Beta\n",
    ),
    (
        "state",
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy: start\n",
    ),
];

/// The same IR must render to the same bytes.
#[test]
fn rendering_twice_produces_identical_bytes() {
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;

        let first = render_term(&ir);
        let second = render_term(&ir);

        assert!(!first.is_empty(), "{name}: rendered nothing");
        assert_eq!(first, second, "{name}: two renders of one IR differ");
    }
}

/// A CLONED IR must render identically.
///
/// Cloning relocates every allocation, so anything keyed on an address orders an equal diagram
/// differently. The repeat test above reuses the same allocations and cannot see it.
#[test]
fn a_cloned_ir_renders_identically() {
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;
        let cloned = ir.clone();

        assert_eq!(
            render_term(&ir),
            render_term(&cloned),
            "{name}: the output depends on WHERE the IR is allocated"
        );
    }
}

/// Lines must stay right-trimmed, and carry no stray control characters.
///
/// NOT asserted here: that every line has the same width. The renderer trims each line
/// (`renderer.rs:2963` writes `line.trim_end()`), so a ragged right edge is DELIBERATE and a
/// rectangularity assertion would fail for a correct reason. I wrote that test first and checked
/// the writer before believing it — a control that fails for a legitimate structural reason is a
/// broken gate, not a finding.
///
/// What the trimming does imply is worth holding down: if a change stopped trimming, every line in
/// every golden would gain invisible trailing spaces and the whole corpus would churn on a diff
/// nobody could see.
#[test]
fn lines_are_trimmed_and_free_of_control_characters() {
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;
        let output = render_term(&ir);

        let lines: Vec<&str> = output.lines().collect();
        assert!(
            lines.len() > 1,
            "{name}: only {} line(s), so this proves nothing",
            lines.len()
        );

        for (index, line) in lines.iter().enumerate() {
            assert_eq!(
                *line,
                line.trim_end(),
                "{name}: line {index} carries trailing whitespace; every golden would churn \
                 invisibly if the writer stopped trimming"
            );
            assert!(
                !line.chars().any(|c| c.is_control()),
                "{name}: line {index} contains a control character, which would corrupt a terminal \
                 or a captured golden"
            );
        }
    }
}

/// No broken float may reach the output as text.
///
/// The terminal writes labels, not coordinates, so a NaN cannot normally appear as a number. If one
/// does, some measurement was formatted into a label -- which is both a visible defect and evidence
/// that arithmetic failed upstream.
#[test]
fn no_broken_float_reaches_the_output() {
    for (name, source) in DIAGRAMS {
        let output = render_term(&fm_parser::parse(source).ir);

        for token in ["NaN", "inf ", "-inf"] {
            assert!(
                !output.contains(token),
                "{name}: {token:?} appears in terminal output, so a failed computation was \
                 formatted into a label"
            );
        }
    }
}

/// CONTROL: the fixtures must render a real grid with drawn content.
///
/// Every assertion above inspects a string. A fixture that failed to parse would render an empty or
/// near-empty grid and satisfy all of them by having nothing to check — and the rectangularity test
/// would pass most convincingly on a single blank line.
#[test]
fn the_fixtures_render_a_real_grid() {
    for (name, source) in DIAGRAMS {
        let output = render_term(&fm_parser::parse(source).ir);
        let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();

        assert!(
            lines.len() >= 3,
            "{name}: only {} non-blank lines, so the scans above are vacuous",
            lines.len()
        );
        assert!(
            output.chars().any(|c| c.is_alphanumeric()),
            "{name}: the grid carries no text at all"
        );
    }
}
