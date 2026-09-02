use fm_layout::LayoutAlgorithm;
fn main() {
    println!("{:>6} {:>11} {:>11} {:>11}", "nodes", "tree_w", "sugi_w", "tree_h");
    for n in [40usize, 60, 80, 100, 110, 120, 140, 200] {
        let mut s = String::from("flowchart TD\n");
        for i in 0..n { s.push_str(&format!("  N{i}[Node {i}]\n")); }
        for i in 1..n { s.push_str(&format!("  N{} --> N{i}\n", i - 1)); }
        let ir = fm_parser::parse(&s).ir;
        let t = fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Tree);
        let u = fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Sugiyama);
        println!("{n:>6} {:>11.1} {:>11.1} {:>11.1}", t.layout.bounds.width, u.layout.bounds.width, t.layout.bounds.height);
    }
}
