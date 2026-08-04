use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let input = std::fs::read_to_string(&path).unwrap();
    let pf = parse(&input);
    let layout = fm_layout::layout_diagram(&pf.ir);
    let svg = render_svg_with_layout(&pf.ir, &layout, &SvgRenderConfig::default());
    let mut h = DefaultHasher::new();
    svg.hash(&mut h);
    let t = fm_layout::layout_diagram_traced(&pf.ir);
    eprintln!(
        "{path}: nodes={} selected={:?} len={} hash={:016x}",
        pf.ir.nodes.len(),
        t.trace.dispatch.selected,
        svg.len(),
        h.finish()
    );
}
