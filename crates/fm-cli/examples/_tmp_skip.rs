use fm_layout::LayoutAlgorithm;
fn main() {
    println!("{:>28} {:>10} {:>11}", "case", "tree_w", "tree_h");
    for (label, skips) in [("chain120 only", 0usize), ("chain120 + 1 skip", 1), ("chain120 + 5 skips", 5), ("chain120 + 11 skips", 11)] {
        let n = 120;
        let mut s = String::from("flowchart TD\n");
        for i in 0..n { s.push_str(&format!("  N{i}[Node {i}]\n")); }
        for i in 1..n { s.push_str(&format!("  N{} --> N{i}\n", i - 1)); }
        for k in 0..skips { s.push_str(&format!("  N{} --> N{}\n", k * 10, k * 10 + 5)); }
        let ir = fm_parser::parse(&s).ir;
        let t = fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Tree);
        println!("{label:>28} {:>10.1} {:>11.1}", t.layout.bounds.width, t.layout.bounds.height);
    }
}
