#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        // Full pipeline: parse → layout → SVG render → terminal render.
        // The primary invariant is: no panics at any stage, for any input.
        let result = fm_parser::parse(input);

        // Confidence must be bounded. Parse-level, so it stays here rather than in the shared
        // layout invariant checker.
        assert!((0.0..=1.0).contains(&result.confidence));

        let layout = fm_layout::layout_diagram(&result.ir);

        // Geometry invariants come from `fm_layout::invariants` rather than being spelled out
        // here, so this target and the `frankenmermaid minimize --signature invariant-violation`
        // reducer test the SAME predicate: an artifact this target rejects is one the reducer can
        // shrink, with no second copy of the rule to drift (bd-2xl.14). It also checks strictly
        // more than the hand-rolled version it replaced, which only looked at node boxes: routed
        // edge points, cluster boxes, cycle-cluster boxes, the diagram bounds, and negative
        // extents are all covered now, and the panic message names the exact field.
        fm_layout::invariants::assert_layout_geometry(&layout);

        // SVG render must not panic.
        let _svg = fm_render_svg::render_svg(&result.ir);

        // Terminal render must not panic.
        let _term = fm_render_term::render_term(&result.ir);
    }
});
