// Is the deep-chain width divergence real INDEPENDENT of the guardrail?
use fm_layout::LayoutAlgorithm;
fn main() {
    // a pure 60-node chain, and the same chain plus a few cross edges (like stress_120_nodes)
    let mut pure = String::from("flowchart TD\n");
    for i in 0..60 { pure.push_str(&format!("  N{i}[Node {i}]\n")); }
    for i in 1..60 { pure.push_str(&format!("  N{} --> N{i}\n", i - 1)); }
    let mut plus = pure.clone();
    for k in 0..5 { plus.push_str(&format!("  N{} --> N{}\n", k * 10, k * 10 + 12)); }
    for (name, src) in [("pure chain(60)", &pure), ("chain+5 cross", &plus)] {
        let ir = fm_parser::parse(src).ir;
        for (a, algo) in [("Tree", LayoutAlgorithm::Tree), ("Sugiyama", LayoutAlgorithm::Sugiyama)] {
            let t = fm_layout::layout_diagram_traced_with_algorithm(&ir, algo);
            println!("{name:16} {a:9} {:>9.1} x {:>9.1}", t.layout.bounds.width, t.layout.bounds.height);
        }
        let au = fm_layout::layout_diagram_traced(&ir);
        println!("{name:16} auto      {:>9.1} x {:>9.1}  ({:?}, {})",
            au.layout.bounds.width, au.layout.bounds.height,
            au.trace.guard.selected_algorithm, au.trace.guard.reason);
        std::fs::write(format!("/data/tmp/claude-1000/-data-projects-frankenmermaid/1bf20e92-25d9-4a88-88c9-c86807212720/scratchpad/ersweep/{}.mmd",
            if name.starts_with("pure") {"chain_pure"} else {"chain_plus"}), src).unwrap();
    }
}
