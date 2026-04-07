use fm_layout::layout_diagram_traced;
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};

fn render_sequence(input: &str) -> String {
    let parsed = parse(input);
    assert!(
        parsed.warnings.is_empty(),
        "Parse errors: {:?}",
        parsed.warnings
    );
    let traced = layout_diagram_traced(&parsed.ir);
    render_svg_with_layout(&parsed.ir, &traced.layout, &SvgRenderConfig::default())
}

#[test]
fn simple_loop_fragment_renders_labeled_rect() {
    let input = "sequenceDiagram\n\
        participant A\n\
        participant B\n\
        loop Every minute\n\
        A->>B: ping\n\
        B-->>A: pong\n\
        end";
    let svg = render_sequence(input);

    assert!(svg.contains("<svg"), "should produce valid SVG");
    assert!(
        svg.contains("fm-sequence-fragment"),
        "should contain fragment class"
    );
    assert!(
        svg.contains("fm-sequence-fragment-label"),
        "should contain fragment label class"
    );
    assert!(
        svg.contains("loop [Every minute]"),
        "should contain loop label text"
    );
}

#[test]
fn alt_else_fragment_renders_both_branches() {
    let input = "sequenceDiagram\n\
        participant A\n\
        participant B\n\
        alt success\n\
        A->>B: ok\n\
        else failure\n\
        A->>B: err\n\
        end";
    let svg = render_sequence(input);

    assert!(svg.contains("<svg"), "should produce valid SVG");
    assert!(
        svg.contains("fm-sequence-fragment"),
        "should contain fragment class"
    );
    assert!(
        svg.contains("alt [success]"),
        "should contain alt label text"
    );
}

#[test]
fn par_fragment_renders_parallel() {
    let input = "sequenceDiagram\n\
        participant A\n\
        participant B\n\
        participant C\n\
        par\n\
        A->>B: one\n\
        and\n\
        A->>C: two\n\
        end";
    let svg = render_sequence(input);

    assert!(svg.contains("<svg"), "should produce valid SVG");
    assert!(
        svg.contains("fm-sequence-fragment"),
        "should contain fragment class"
    );
}

#[test]
fn nested_fragments_render_multiple_rects() {
    let input = "sequenceDiagram\n\
        participant A\n\
        participant B\n\
        loop repeat\n\
        alt success\n\
        A->>B: yes\n\
        else fail\n\
        A->>B: no\n\
        end\n\
        end";
    let svg = render_sequence(input);

    assert!(svg.contains("<svg"), "should produce valid SVG");
    // Count fragment rectangles (dashed or solid).
    let fragment_count = svg.matches("fm-sequence-fragment\"").count()
        + svg.matches("fm-sequence-fragment ").count();
    assert!(
        fragment_count >= 2,
        "nested fragments should produce at least 2 fragment rects, got {fragment_count}"
    );
}

#[test]
fn fragment_geometry_has_nonzero_bounds() {
    let input = "sequenceDiagram\n\
        participant A\n\
        participant B\n\
        loop Retry\n\
        A->>B: request\n\
        B-->>A: response\n\
        end";
    let parsed = parse(input);
    let traced = layout_diagram_traced(&parsed.ir);
    let fragments = &traced.layout.extensions.sequence_fragments;

    assert!(
        !fragments.is_empty(),
        "layout should produce sequence fragments"
    );

    for fragment in fragments {
        assert!(fragment.bounds.width > 0.0, "fragment width should be positive");
        assert!(
            fragment.bounds.height > 0.0,
            "fragment height should be positive"
        );
    }
}
