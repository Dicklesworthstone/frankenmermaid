use fm_parser::parse;
fn mm(n: usize) -> String {
    let mut l = vec!["mindmap".to_string(), "  root".to_string()];
    for i in 0..n { l.push(format!("    child{i}")); }
    l.join("\n")
}
fn main() {
    for n in [5usize, 30, 33, 40, 100, 200, 800] {
        let pf = parse(&mm(n));
        let t = fm_layout::layout_diagram_traced(&pf.ir);
        eprintln!("n={n:4}: selected={:?} reason={}", t.trace.dispatch.selected, t.trace.dispatch.reason);
    }
}
