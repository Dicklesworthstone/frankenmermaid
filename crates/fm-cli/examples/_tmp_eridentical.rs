// Does the guardrail's forced algorithm change the OUTPUT at all on this lane?
use fm_layout::LayoutAlgorithm;
fn main() {
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let item = corpus.as_array().map_or(corpus.clone(), |a| a[0].clone());
    let texts: Vec<String> = item["texts"].as_array().unwrap().iter()
        .map(|t| t.as_str().unwrap().to_string()).collect();
    let cfg = fm_render_svg::SvgRenderConfig::default();
    let (mut same, mut diff) = (0usize, 0usize);
    for t in &texts {
        let ir = fm_parser::parse(t).ir;
        let tree = fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Tree);
        let sugi = fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Sugiyama);
        let a = fm_render_svg::render_svg_with_layout(&ir, &tree.layout, &cfg);
        let b = fm_render_svg::render_svg_with_layout(&ir, &sugi.layout, &cfg);
        if a == b { same += 1; } else { diff += 1; }
    }
    println!("revisions={} svg byte-identical(Tree vs Sugiyama)={} differing={}", texts.len(), same, diff);
}
