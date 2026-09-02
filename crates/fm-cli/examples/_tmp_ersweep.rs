// Emit every revision where Tree and Sugiyama differ, plus both arms' bounds, for incumbent compare.
use fm_layout::LayoutAlgorithm;
fn main() {
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let item = corpus.as_array().map_or(corpus.clone(), |a| a[0].clone());
    let texts: Vec<String> = item["texts"].as_array().unwrap().iter()
        .map(|t| t.as_str().unwrap().to_string()).collect();
    let dir = std::env::args().nth(2).unwrap();
    let cfg = fm_render_svg::SvgRenderConfig::default();
    let mut rows = Vec::new();
    for (i, t) in texts.iter().enumerate() {
        let ir = fm_parser::parse(t).ir;
        let tr = fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Tree);
        let su = fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Sugiyama);
        if fm_render_svg::render_svg_with_layout(&ir, &tr.layout, &cfg)
            == fm_render_svg::render_svg_with_layout(&ir, &su.layout, &cfg) { continue; }
        std::fs::write(format!("{dir}/rev{i}.mmd"), t).unwrap();
        rows.push(serde_json::json!({"rev": i,
            "tree_w": tr.layout.bounds.width, "tree_h": tr.layout.bounds.height,
            "sugi_w": su.layout.bounds.width, "sugi_h": su.layout.bounds.height}));
        if rows.len() >= 8 { break; }
    }
    println!("{}", serde_json::to_string(&rows).unwrap());
}
