use fm_layout::{LayoutAlgorithm, LayoutGuardrails};
fn main() {
    let big = LayoutGuardrails { max_layout_time_ms: usize::MAX/4, max_layout_iterations: usize::MAX/4, max_route_ops: usize::MAX/4 };
    for (name, path) in [
        ("stress_120_nodes", "crates/fm-cli/tests/golden/stress_120_nodes.mmd"),
        ("er rev1", "/data/tmp/claude-1000/-data-projects-frankenmermaid/1bf20e92-25d9-4a88-88c9-c86807212720/scratchpad/c4probe/erdiff.mmd"),
    ] {
        let ir = fm_parser::parse(&std::fs::read_to_string(path).unwrap()).ir;
        println!("--- {name} (nodes={} edges={})", ir.nodes.len(), ir.edges.len());
        for (a, algo) in [("Tree", LayoutAlgorithm::Tree), ("Sugiyama", LayoutAlgorithm::Sugiyama)] {
            let g = fm_layout::layout_diagram_traced_with_algorithm(&ir, algo);
            let u = fm_layout::layout_diagram_traced_with_algorithm_and_guardrails(&ir, algo, big);
            println!("  {a:9} guarded {:>9.1}x{:<9.1} [{:?}]   unguarded {:>9.1}x{:<9.1} [{:?}]",
                g.layout.bounds.width, g.layout.bounds.height, g.trace.guard.selected_algorithm,
                u.layout.bounds.width, u.layout.bounds.height, u.trace.guard.selected_algorithm);
        }
    }
}
