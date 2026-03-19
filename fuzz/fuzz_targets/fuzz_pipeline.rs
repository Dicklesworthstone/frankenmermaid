#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        // Full pipeline: parse → layout → SVG render → terminal render.
        // The primary invariant is: no panics at any stage, for any input.
        let result = fm_parser::parse(input);

        // Confidence must be bounded.
        assert!((0.0..=1.0).contains(&result.confidence));

        let layout = fm_layout::layout_diagram(&result.ir);

        // All layout node coordinates must be finite.
        for node in &layout.nodes {
            assert!(node.bounds.x.is_finite());
            assert!(node.bounds.y.is_finite());
            assert!(node.bounds.width.is_finite());
            assert!(node.bounds.height.is_finite());
        }

        // SVG render must not panic.
        let _svg = fm_render_svg::render_svg(&result.ir);

        // Terminal render must not panic.
        let _term = fm_render_term::render_term(&result.ir);
    }
});
