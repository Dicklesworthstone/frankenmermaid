//! The canvas op stream must be deterministic and free of broken floats (bd-1s1g.6).
//!
//! This is the wasm32 leg of that bead, tested where it can actually be run. The canvas path is what
//! fm-wasm ships to a browser, so its arithmetic is the arithmetic that executes on wasm32 — and
//! wasm32 is strict IEEE f64 with no x87 extended precision and no FMA contraction, which means a
//! divergence between this path and a native run is far more likely to come from ORDER (iteration,
//! accumulation, allocation) than from rounding. Order is exactly what these tests attack, and they
//! attack it on the host, where a failure is cheap to see.
//!
//! Complements rather than repeats the other suites: fm-layout covers shape geometry, fm-cli covers
//! solver coordinates, fm-render-svg covers the serialised SVG. None of them touches the canvas
//! operation stream, which is a separate writer with its own arithmetic.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

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

/// Record one render as its operation stream.
fn ops_for(ir: &fm_core::MermaidDiagramIr) -> String {
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

/// The same IR must produce the same operations, in the same order.
///
/// Order is the substance here, not just the values: a canvas is an imperative stream, so two
/// renders that draw the same shapes in a different sequence paint different pixels wherever
/// anything overlaps.
#[test]
fn rendering_twice_produces_an_identical_operation_stream() {
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;

        let first = ops_for(&ir);
        let second = ops_for(&ir);

        assert!(!first.is_empty(), "{name}: recorded no operations");
        assert_eq!(
            first, second,
            "{name}: two renders of one IR produced different operation streams"
        );
    }
}

/// A CLONED IR must produce the same stream.
///
/// Cloning moves every allocation, so anything keyed on an address orders an equal diagram
/// differently — invisible to the repeat test above, which reuses the same allocations.
#[test]
fn a_cloned_ir_produces_an_identical_operation_stream() {
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;
        let cloned = ir.clone();

        assert_eq!(
            ops_for(&ir),
            ops_for(&cloned),
            "{name}: the operation stream depends on WHERE the IR is allocated"
        );
    }
}

/// No operation may carry NaN, infinity, or negative zero.
///
/// A canvas silently ignores a draw call with a NaN coordinate — nothing is painted, nothing is
/// logged, and the diagram is simply missing an element. That is strictly worse than an SVG, where
/// the bad value at least survives in the document for someone to find.
///
/// Negative zero is included for the same reason it is checked in the SVG suite: it compares EQUAL
/// to zero, so no numeric assertion anywhere can see it, and it changes the recorded stream.
#[test]
fn no_operation_carries_a_broken_float() {
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;
        let ops = ops_for(&ir);

        // Rust's float Debug spells these exactly `NaN`, `inf` and `-inf`, and they are matched
        // WITH a delimiter rather than as bare substrings. A lowercase `contains("inf")` would fire
        // on any label containing "info" -- a false failure that the next person relaxes, on a check
        // whose whole value is that it never fires spuriously.
        for terminator in [',', ')', ' '] {
            for token in [format!("NaN{terminator}"), format!("inf{terminator}")] {
                assert!(
                    !ops.contains(&token),
                    "{name}: an operation carries {token:?}; a canvas ignores such a draw call \
                     silently, so the element would simply be missing"
                );
            }
        }
        for terminator in [',', ')', ' '] {
            let needle = format!("-0.0{terminator}");
            // Name the OPERATION, not just the needle. "somewhere in this diagram there is a
            // negative zero" sends the reader back to the renderer to search by hand; the op tells
            // them which call produced it, which is the whole question when the value could have
            // come from layout or from this crate's own arithmetic.
            let culprit = ops
                .split("), ")
                .find(|op| op.contains(&needle))
                .unwrap_or("<not found in any single op>");
            assert!(
                !ops.contains(&needle),
                "{name}: an operation carries negative zero ({needle:?}), which no numeric \
                 assertion can distinguish from 0.0 -- in: {culprit}"
            );
        }
    }
}

/// CONTROL: the fixtures must produce a substantial stream with real drawing in it.
///
/// Every assertion above scans a string. A fixture that failed to parse would record a near-empty
/// stream and satisfy all of them by having nothing to find — and the NaN scan in particular would
/// look strongest exactly when it was checking least.
#[test]
fn the_fixtures_record_real_drawing() {
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;
        let ops = ops_for(&ir);

        assert!(
            ops.len() > 200,
            "{name}: recorded only {} bytes of operations, so the scans are vacuous",
            ops.len()
        );
        assert!(
            ops.contains("FillText") || ops.contains("Stroke") || ops.contains("Fill"),
            "{name}: the stream contains no drawing operations at all"
        );
    }
}
