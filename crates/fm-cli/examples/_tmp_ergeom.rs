use fm_layout::LayoutAlgorithm;
fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let ir = fm_parser::parse(&src).ir;
    println!("nodes={} edges={}", ir.nodes.len(), ir.edges.len());
    for (name, algo) in [("Tree", LayoutAlgorithm::Tree), ("Sugiyama", LayoutAlgorithm::Sugiyama)] {
        let t = fm_layout::layout_diagram_traced_with_algorithm(&ir, algo);
        println!("{name:9} {:>9.1} x {:>9.1}  aspect 1:{:.2}",
            t.layout.bounds.width, t.layout.bounds.height,
            t.layout.bounds.height / t.layout.bounds.width.max(1.0));
    }
    let a = fm_layout::layout_diagram_traced(&ir);
    println!("auto      {:>9.1} x {:>9.1}  picks {:?} (reason {})",
        a.layout.bounds.width, a.layout.bounds.height, a.trace.guard.selected_algorithm, a.trace.guard.reason);
}
