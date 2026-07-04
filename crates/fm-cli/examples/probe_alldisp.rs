use fm_parser::parse;
mod g {
    include!("gen_shared.rs");
}
fn main() {
    // (shape, dedicated algo name expected)
    let shapes = [
        "seq", "gantt", "sankey", "state", "pie", "git", "block", "class", "er", "mindmap",
    ];
    for shape in shapes {
        for n in [30usize, 200] {
            let input = g::gen_input(shape, n);
            let pf = parse(&input);
            let t = fm_layout::layout_diagram_traced(&pf.ir);
            eprintln!(
                "{shape:8} n={n:4} type={:?} selected={:?} reason={}",
                pf.ir.diagram_type, t.trace.dispatch.selected, t.trace.dispatch.reason
            );
        }
    }
}
