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

        // ⚠️ A TRAILING DELIMITER IS NOT ENOUGH, and this assertion FAILED ON ITS OWN FIXTURE for
        // that reason. The comment here used to say `-0` followed by a delimiter "is the only shape
        // that means the number itself was negative zero". It is not: measured on the `flowchart`
        // fixture, `-0"` occurs 11 times and NONE of them is a number —
        //
        //     data-fm-source-span="2:1-2:22@0-0"   the byte RANGE separator, 9 of them
        //     id="fm-edge-0"  id="fm-node-a-0"     an index SUFFIX on an identifier, 2 of them
        //
        // A number needs a delimiter on BOTH sides: something must end before it starts. With the
        // leading delimiter required, the same fixture matches ZERO times, so the check keeps its
        // teeth and loses the false failure.
        //
        // This is the same trap the `inf` check above already documents — a bare `contains("inf")`
        // firing on "info" — and it is worth noticing that the author saw it for one needle and not
        // for the neighbouring one. Explicit substrings are still the right style here; they just
        // have to spell the whole shape.
        for start in ['"', ' ', ',', '(', ':'] {
            for terminator in ['"', ' ', ',', ')', ';'] {
                let needle = format!("{start}-0{terminator}");
                assert!(
                    !svg.contains(&needle),
                    "{name}: serialised negative zero ({needle:?}); it compares EQUAL to 0 so no \
                     numeric assertion can catch it, and it rewrites every diff it appears in"
                );
            }
        }

        // PATH DATA NEEDS ITS OWN PASS, because there a number may abut a command letter with no
        // delimiter between them: this renderer emits `d="M0 0 L8 3.50 Z"`, so a negative zero
        // would appear as `M-0`, which the both-sides rule above cannot see.
        //
        // Adding the command letters to the `start` set instead would have reintroduced the very
        // false positive that rule exists to remove — `a` is a path command AND the last letter of
        // `id="fm-node-a-0"`. Scanning only INSIDE `d="…"` keeps the two questions apart: no
        // identifier appears there, so a leading letter is unambiguously a command.
        // ⚠️ SPLIT ON ` d="`, WITH THE LEADING SPACE. `d="` alone also matches the tail of `id="`,
        // and my first version of this loop duly "found" negative zeros in `id="fm-edge-0"` and
        // `id="fm-node-a-0"` — the same false positive, one layer down, in the code written to
        // remove it. Anchoring on the attribute boundary takes it to zero on this fixture.
        for path in svg
            .split(" d=\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
        {
            for (index, _) in path.match_indices("-0") {
                let after = path[index + 2..].chars().next();
                // The `-0` is a WHOLE number only if nothing numeric follows it; `-0.5` and
                // `-05` are ordinary values that happen to start this way.
                let is_bare_negative_zero =
                    !matches!(after, Some(c) if c.is_ascii_digit() || c == '.');
                assert!(
                    !is_bare_negative_zero,
                    "{name}: serialised negative zero in path data ({path:?}); it compares EQUAL \
                     to 0 so no numeric assertion can catch it"
                );
            }
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
