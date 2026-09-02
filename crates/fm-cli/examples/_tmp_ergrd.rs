// Is the guardrail's forced fallback actually the FASTER algorithm on this workload?
use std::time::Instant;
use fm_layout::LayoutAlgorithm;
fn main() {
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let item = corpus.as_array().map_or(corpus.clone(), |a| a[0].clone());
    let texts: Vec<String> = item["texts"].as_array().unwrap().iter()
        .map(|t| t.as_str().unwrap().to_string()).collect();
    let irs: Vec<_> = texts.iter().map(|t| fm_parser::parse(t).ir).collect();

    for (name, algo) in [("Tree", LayoutAlgorithm::Tree), ("Sugiyama", LayoutAlgorithm::Sugiyama)] {
        // warmup
        for ir in &irs { let _ = fm_layout::layout_diagram_traced_with_algorithm(ir, algo); }
        let mut best = u128::MAX;
        for _ in 0..7 {
            let t = Instant::now();
            for ir in &irs { std::hint::black_box(fm_layout::layout_diagram_traced_with_algorithm(ir, algo)); }
            best = best.min(t.elapsed().as_nanos());
        }
        println!("{name:9} best-of-7 over {} revisions: {:>8.3} ms", irs.len(), best as f64 / 1e6);
    }
    // what the guardrail actually picks today
    let tr = fm_layout::layout_diagram_traced(&irs[0]);
    println!("guardrail picks: {:?} (initial {:?}, reason {})",
        tr.trace.guard.selected_algorithm, tr.trace.guard.initial_algorithm, tr.trace.guard.reason);
}
