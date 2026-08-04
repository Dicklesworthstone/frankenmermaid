use fm_parser::parse;
use std::time::Instant;
fn mm(n: usize) -> String {
    let mut l = vec!["mindmap".to_string(), "  root".to_string()];
    for i in 0..n {
        l.push(format!("    child{i}"));
    }
    l.join("\n")
}
fn best<F: Fn() -> (usize, bool)>(f: F, iters: usize) -> (u128, usize, bool) {
    let mut b = u128::MAX;
    let mut nodes = 0;
    let mut fin = true;
    for _ in 0..iters {
        let t = Instant::now();
        let (n, fi) = f();
        let e = t.elapsed().as_nanos();
        if e < b {
            b = e;
        }
        nodes = n;
        fin = fi;
    }
    (b, nodes, fin)
}
fn main() {
    for n in [50usize, 200, 800, 1600] {
        let pf = parse(&mm(n));
        let ir = &pf.ir;
        let (rt, rn, rf) = best(
            || {
                let l = fm_layout::layout_diagram_radial(ir);
                (
                    l.nodes.len(),
                    l.nodes.iter().all(|nb| nb.bounds.x.is_finite()),
                )
            },
            200,
        );
        // Tree direct (bypass guardrail) via traced_with_algorithm_and_guardrails with huge budget
        let g = fm_layout::LayoutGuardrails {
            max_layout_time_ms: usize::MAX,
            max_layout_iterations: usize::MAX,
            max_route_ops: usize::MAX,
        };
        let (tt, tn, tf) = best(
            || {
                let tr = fm_layout::layout_diagram_traced_with_algorithm_and_guardrails(
                    ir,
                    fm_layout::LayoutAlgorithm::Tree,
                    g,
                );
                (
                    tr.layout.nodes.len(),
                    tr.layout.nodes.iter().all(|nb| nb.bounds.x.is_finite()),
                )
            },
            200,
        );
        eprintln!(
            "n={n:4}: Radial(direct) min={rt:>10}ns nodes={rn} finite={rf} | Tree(direct) min={tt:>10}ns nodes={tn} finite={tf} | Tree/Radial={:.1}x",
            tt as f64 / rt as f64
        );
    }
}
