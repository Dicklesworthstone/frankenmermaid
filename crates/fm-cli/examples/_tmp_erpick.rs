use fm_layout::LayoutAlgorithm;
fn main() {
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let item = corpus.as_array().map_or(corpus.clone(), |a| a[0].clone());
    let texts: Vec<String> = item["texts"].as_array().unwrap().iter()
        .map(|t| t.as_str().unwrap().to_string()).collect();
    let cfg = fm_render_svg::SvgRenderConfig::default();
    for (i, t) in texts.iter().enumerate() {
        let ir = fm_parser::parse(t).ir;
        let tr = fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Tree);
        let su = fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Sugiyama);
        let a = fm_render_svg::render_svg_with_layout(&ir, &tr.layout, &cfg);
        let b = fm_render_svg::render_svg_with_layout(&ir, &su.layout, &cfg);
        if a != b {
            println!("first differing revision = {i}  nodes={} edges={}", ir.nodes.len(), ir.edges.len());
            println!("  Tree     {:>9.1} x {:>9.1}", tr.layout.bounds.width, tr.layout.bounds.height);
            println!("  Sugiyama {:>9.1} x {:>9.1}", su.layout.bounds.width, su.layout.bounds.height);
            std::fs::write(std::env::args().nth(2).unwrap(), t).unwrap();
            return;
        }
    }
}
