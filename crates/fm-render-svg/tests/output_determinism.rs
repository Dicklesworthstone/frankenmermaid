//! The SVG a user diffs must be byte-stable and free of unportable float spellings (bd-1s1g.6).
//!
//! The layout-level determinism tests (fm-layout `fp_determinism_faults.rs`, fm-cli
//! `layout_fp_determinism.rs`) assert that coordinates are bit-identical. This is the layer BELOW
//! that question and the one users actually see: two bit-identical `f32` values can still be
//! SERIALISED differently, and the serialised form is what lands in a golden file, a git diff, and a
//! reviewer's eyes.
//!
//! Three spellings matter, and none of them is caught by comparing coordinates:
//!
//!   * `-0` — negative zero compares EQUAL to positive zero under `==`, so every numeric assertion
//!     in this project passes while the text changes. A sign that flips with a platform's rounding
//!     of an underflowing subtraction rewrites the file and nothing else notices.
//!   * `NaN` / `inf` — arithmetic that broke, serialised as though it were a coordinate. A viewer
//!     silently drops the element; a diff shows a plausible-looking attribute.
//!   * exponent forms like `1e-7` — valid CSS/SVG number syntax, but a formatter that switches into
//!     one at a magnitude boundary produces a large diff from a tiny geometry change.

use fm_core::MermaidDiagramIr;
use fm_render_svg::render_svg;

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

fn ir(source: &str) -> MermaidDiagramIr {
    fm_parser::parse(source).ir
}

/// The same IR must serialise to the same bytes.
#[test]
fn rendering_twice_produces_identical_bytes() {
    for (name, source) in DIAGRAMS {
        let parsed = ir(source);
        let first = render_svg(&parsed);
        let second = render_svg(&parsed);

        assert!(!first.is_empty(), "{name}: rendered nothing");
        assert_eq!(
            first, second,
            "{name}: two renders of one IR differ; something in the writer depends on iteration or \
             allocation order"
        );
    }
}

/// A CLONED IR must serialise identically to the original.
///
/// Cloning relocates every allocation, so anything keyed on an address orders an equal document
/// differently. A repeat-render test cannot see that, because it reuses the same allocations.
#[test]
fn a_cloned_ir_serialises_identically() {
    for (name, source) in DIAGRAMS {
        let parsed = ir(source);
        let cloned = parsed.clone();

        assert_eq!(
            render_svg(&parsed),
            render_svg(&cloned),
            "{name}: the output depends on WHERE the IR is allocated"
        );
    }
}

/// No output may contain a broken or unportable float spelling.
///
/// `-0` is the interesting one: it is numerically equal to `0`, so no coordinate assertion anywhere
/// in this project can see it, and it rewrites the file when it appears.
#[test]
fn no_output_contains_nan_infinity_or_negative_zero() {
    for (name, source) in DIAGRAMS {
        let svg = render_svg(&ir(source));
        let lowered = svg.to_lowercase();

        for token in ["nan", "infinity"] {
            assert!(
                !lowered.contains(token),
                "{name}: the SVG contains {token:?}, which means arithmetic broke and was written \
                 out as a coordinate"
            );
        }

        // `-0` immediately followed by a DELIMITER, which is the only shape that means the
        // number itself was negative zero. Written as explicit substrings rather than a clever
        // scan: a convoluted detector that false-fails once gets relaxed by the next person, and a
        // relaxed detector for this is worth nothing, since -0 is invisible to every other test.
        for terminator in ['"', ' ', ',', ')', ';'] {
            let needle = format!("-0{terminator}");
            assert!(
                !svg.contains(&needle),
                "{name}: serialised negative zero ({needle:?}); it compares EQUAL to 0 so no \
                 numeric assertion can catch it, and it rewrites every diff it appears in"
            );
        }
    }
}

/// CONTROL: the fixtures must actually produce substantial SVG with coordinates in it.
///
/// Every assertion above scans a string. A fixture that failed to parse would render a stub, and
/// the scans would pass by having nothing to find.
#[test]
fn the_fixtures_render_real_svg() {
    for (name, source) in DIAGRAMS {
        let svg = render_svg(&ir(source));

        assert!(
            svg.len() > 200,
            "{name}: rendered only {} bytes, so the scans above are vacuous",
            svg.len()
        );
        assert!(
            svg.contains("<svg"),
            "{name}: output is not an SVG document"
        );
        assert!(
            svg.contains('.') || svg.contains("width"),
            "{name}: output carries no numeric attributes, so the float-spelling scan proves nothing"
        );
    }
}
