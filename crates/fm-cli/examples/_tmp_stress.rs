use fm_layout::LayoutAlgorithm;
fn main() {
    let src = std::fs::read_to_string("crates/fm-cli/tests/golden/stress_120_nodes.mmd").unwrap();
    let ir = fm_parser::parse(&src).ir;
    println!("nodes={} edges={}", ir.nodes.len(), ir.edges.len());
    for (name, algo) in [("Tree", LayoutAlgorithm::Tree), ("Sugiyama", LayoutAlgorithm::Sugiyama)] {
        let t = fm_layout::layout_diagram_traced_with_algorithm(&ir, algo);
        println!("{name:9} {:>10.1} x {:>10.1}  (w x h)  aspect 1:{:.1}",
            t.layout.bounds.width, t.layout.bounds.height,
            t.layout.bounds.height / t.layout.bounds.width.max(1.0));
    }
    let auto = fm_layout::layout_diagram_traced(&ir);
    println!("auto      {:>10.1} x {:>10.1}  picks {:?} (initial {:?}, reason {})",
        auto.layout.bounds.width, auto.layout.bounds.height,
        auto.trace.guard.selected_algorithm, auto.trace.guard.initial_algorithm, auto.trace.guard.reason);
    println!("blessed   {:>10.1} x {:>10.1}  (recorded golden)", 245.875, 22364.0);
}
