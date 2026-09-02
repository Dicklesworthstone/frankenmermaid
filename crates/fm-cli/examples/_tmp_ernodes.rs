fn main() {
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let item = corpus.as_array().map_or(corpus.clone(), |a| a[0].clone());
    let t = item["texts"].as_array().unwrap()[0].as_str().unwrap();
    let ir = fm_parser::parse(t).ir;
    let (n, e) = (ir.nodes.len(), ir.edges.len());
    let est = n * 4 + e * 2 + 8;
    println!("revision 0: nodes={n} edges={e}");
    println!("Tree generic estimate = nodes*4 + edges*2 + 8 = {est} ms");
    println!("measured Tree total for 25 revisions = 3.278 ms  ->  {:.4} ms per revision", 3.278/25.0);
    println!("estimate overstates a single revision by {:.0}x", est as f64 / (3.278/25.0));
    println!("per-node: model charges 4.000 ms, actual {:.5} ms  -> {:.0}x", (3.278/25.0)/n as f64, 4.0/((3.278/25.0)/n as f64));
}
