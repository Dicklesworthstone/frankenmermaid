// Characterise Tree layout's ACTUAL cost curve so the guardrail constant can be FIT, not guessed.
use std::time::Instant;
use fm_layout::LayoutAlgorithm;

fn er_schema(entities: usize, attrs: usize) -> String {
    let mut s = String::from("erDiagram\n");
    for i in 0..entities {
        s.push_str(&format!("  E{i} {{\n"));
        for a in 0..attrs { s.push_str(&format!("    string f{a}\n")); }
        s.push_str("  }\n");
    }
    for i in 1..entities { s.push_str(&format!("  E{} ||--o{{ E{i} : r\n", i - 1)); }
    s
}

fn main() {
    println!("{:>7} {:>7} {:>11} {:>13} {:>11}", "nodes", "edges", "tree_ms", "ms_per_node", "old_est_ms");
    for ents in [10usize, 25, 50, 75, 150, 300, 600] {
        let src = er_schema(ents, 3);
        let ir = fm_parser::parse(&src).ir;
        let (n, e) = (ir.nodes.len(), ir.edges.len());
        for _ in 0..2 { let _ = fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Tree); }
        let mut best = u128::MAX;
        for _ in 0..5 {
            let t = Instant::now();
            std::hint::black_box(fm_layout::layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Tree));
            best = best.min(t.elapsed().as_nanos());
        }
        let ms = best as f64 / 1e6;
        println!("{n:>7} {e:>7} {ms:>11.4} {:>13.6} {:>11}", ms / n as f64, n * 4 + e * 2 + 8);
    }
}
