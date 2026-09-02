// Dump the guard's selected algorithm for every golden fixture, so a constant change can be
// diffed against it selection-by-selection.
fn main() {
    let dir = "crates/fm-cli/tests/golden";
    let mut names: Vec<_> = std::fs::read_dir(dir).unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
        .filter(|n| n.ends_with(".mmd")).collect();
    names.sort();
    for n in names {
        let src = match std::fs::read_to_string(format!("{dir}/{n}")) { Ok(s) => s, Err(_) => continue };
        let ir = fm_parser::parse(&src).ir;
        let t = fm_layout::layout_diagram_traced(&ir);
        println!("{:<44} {:>5}n {:>5}e  sel={:?} init={:?} est={} reason={}",
            n.trim_end_matches(".mmd"), ir.nodes.len(), ir.edges.len(),
            t.trace.guard.selected_algorithm, t.trace.guard.initial_algorithm,
            t.trace.guard.estimated_layout_time_ms, t.trace.guard.reason);
    }
}
