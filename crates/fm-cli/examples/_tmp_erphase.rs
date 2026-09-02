// Phase-split schema_catalog_25's dominant lane and show the layout decision that governs it.
use std::time::Instant;
fn main() {
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::env::args().nth(1).expect("corpus path")).unwrap(),
    )
    .unwrap();
    let item = corpus.as_array().map_or(corpus.clone(), |a| a[0].clone());
    let texts: Vec<String> = item["texts"].as_array().unwrap().iter()
        .map(|t| t.as_str().unwrap().to_string()).collect();
    println!("revisions={}", texts.len());

    let (mut p, mut l, mut r) = (0u128, 0u128, 0u128);
    let mut bytes = 0usize;
    let mut algo = String::new();
    let mut guard = String::new();
    for t in &texts {
        let t0 = Instant::now();
        let ir = fm_parser::parse(t).ir;
        p += t0.elapsed().as_nanos();
        let t1 = Instant::now();
        let traced = fm_layout::layout_diagram_traced(&ir);
        l += t1.elapsed().as_nanos();
        if algo.is_empty() {
            algo = format!("{:?}", traced.trace.dispatch.selected);
            guard = format!("{:?}", traced.trace.guard);
        }
        let t2 = Instant::now();
        let svg = fm_render_svg::render_svg_with_layout(&ir, &traced.layout, &Default::default());
        r += t2.elapsed().as_nanos();
        bytes += svg.len();
    }
    let tot = (p + l + r) as f64;
    println!("parse  {:>8.3} ms  {:>5.1}%", p as f64 / 1e6, 100.0 * p as f64 / tot);
    println!("layout {:>8.3} ms  {:>5.1}%", l as f64 / 1e6, 100.0 * l as f64 / tot);
    println!("render {:>8.3} ms  {:>5.1}%", r as f64 / 1e6, 100.0 * r as f64 / tot);
    println!("total  {:>8.3} ms  bytes={bytes}", tot / 1e6);
    println!("algorithm={algo}");
    println!("guard={guard}");
}
