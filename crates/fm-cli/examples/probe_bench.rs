use fm_parser::parse;
use std::time::Instant;
fn flowchart(n: usize) -> String {
    let mut l = vec!["flowchart LR".to_string()];
    for i in 0..n.saturating_sub(1) {
        l.push(format!("  N{i}-->N{}", i + 1));
    }
    l.join("\n")
}
fn er(n: usize) -> String {
    let mut l = vec!["erDiagram".to_string()];
    for i in 0..n.saturating_sub(1) {
        l.push(format!("  E{i} ||--o{{ E{} : rel", i + 1));
    }
    l.join("\n")
}
fn seq(n: usize) -> String {
    let mut l = vec!["sequenceDiagram".to_string()];
    for i in 0..n {
        l.push(format!("  A{}->>A{}: m{i}", i % 5, (i + 1) % 5));
    }
    l.join("\n")
}
fn bench(name: &str, input: &str) {
    for _ in 0..50 {
        std::hint::black_box(parse(input));
    }
    let iters = 2000;
    let mut best = u128::MAX;
    for _ in 0..5 {
        let t = Instant::now();
        for _ in 0..iters {
            std::hint::black_box(parse(input));
        }
        let ns = t.elapsed().as_nanos() / iters;
        if ns < best {
            best = ns;
        }
    }
    eprintln!(
        "FM {name}: {:.1}us bytes={}",
        best as f64 / 1000.0,
        input.len()
    );
}
fn main() {
    bench("flowchart 200", &flowchart(200));
    bench("er 200", &er(200));
    bench("sequence 200", &seq(200));
}
