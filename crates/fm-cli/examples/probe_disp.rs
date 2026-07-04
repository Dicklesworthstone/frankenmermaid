use fm_parser::parse;
fn main() {
    let n = 2000;
    let mut l = vec!["mindmap".to_string(), "  root".to_string()];
    for i in 0..n {
        l.push(format!("    child{i}"));
    }
    let input = l.join("\n");
    let pf = parse(&input);
    eprintln!(
        "diagram_type={:?} nodes={} edges={}",
        pf.ir.diagram_type,
        pf.ir.nodes.len(),
        pf.ir.edges.len()
    );
    let t = fm_layout::layout_diagram_traced(&pf.ir);
    eprintln!(
        "selected={:?} reason={}",
        t.trace.dispatch.selected, t.trace.dispatch.reason
    );
}
