use fm_parser::parse;
mod g { include!("gen_shared.rs"); }
use fm_layout::{LayoutAlgorithm, LayoutGuardrails, layout_diagram_traced, layout_diagram_traced_with_algorithm_and_guardrails};
use std::time::Instant;
fn bestn<F: Fn() -> (usize,bool)>(f: F, it: usize) -> (u128,usize,bool) { let mut b=u128::MAX; let mut n=0; let mut fin=true; for _ in 0..it { let t=Instant::now(); let (nn,ff)=f(); let e=t.elapsed().as_nanos(); if e<b {b=e;} n=nn; fin=ff; } (b,n,fin) }
fn main() {
    let huge = LayoutGuardrails { max_layout_time_ms: usize::MAX, max_layout_iterations: usize::MAX, max_route_ops: usize::MAX };
    // (shape, dedicated algo)
    let cases = [("seq", LayoutAlgorithm::Sequence), ("sankey", LayoutAlgorithm::Sankey), ("pie", LayoutAlgorithm::Pie), ("block", LayoutAlgorithm::Grid)];
    for (shape, ded) in cases {
        for n in [200usize] {
            let pf = parse(&g::gen_input(shape, n));
            let ir = &pf.ir;
            // current dispatch (with default guardrail)
            let (ct, cn, cf) = bestn(|| { let t=layout_diagram_traced(ir); (t.layout.nodes.len(), t.layout.nodes.iter().all(|b| b.bounds.x.is_finite())) }, 300);
            let cur_algo = layout_diagram_traced(ir).trace.dispatch.selected;
            // dedicated direct (bypass guardrail)
            let (dt, dn, df) = bestn(|| { let t=layout_diagram_traced_with_algorithm_and_guardrails(ir, ded, huge); (t.layout.nodes.len(), t.layout.nodes.iter().all(|b| b.bounds.x.is_finite())) }, 300);
            eprintln!("{shape:7} n={n}: CURRENT({cur_algo:?}) {ct:>9}ns nodes={cn} fin={cf} | DEDICATED({ded:?}) {dt:>9}ns nodes={dn} fin={df} | dedicated is {:.2}x current", ct as f64/dt as f64);
        }
    }
}
