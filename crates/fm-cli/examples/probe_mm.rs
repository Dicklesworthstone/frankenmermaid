use fm_parser::parse;
fn check(name: &str, input: &str) {
    let pf = parse(input);
    let layout = fm_layout::layout_diagram(&pf.ir);
    let mut pts = std::collections::BTreeMap::new();
    let mut nudged = 0;
    for e in &layout.edges {
        *pts.entry(e.points.len()).or_insert(0) += 1;
        if e.points.len() > 2 {
            nudged += 1;
        }
    }
    eprintln!(
        "{name}: edges={} point_count_histogram={:?} nudged(>2pts)={}",
        layout.edges.len(),
        pts,
        nudged
    );
}
fn main() {
    // flat star
    let mut s = String::from("mindmap\n  root\n");
    for i in 0..300 {
        s.push_str(&format!("    child{i}\n"));
    }
    check("mm_star_300", &s);
    // balanced 2-level
    let mut s = String::from("mindmap\n  root\n");
    for i in 0..17 {
        s.push_str(&format!("    b{i}\n"));
        for j in 0..17 {
            s.push_str(&format!("      leaf{i}_{j}\n"));
        }
    }
    check("mm_balanced_~300", &s);
    // small
    check(
        "mm_small",
        "mindmap\n  root\n    Origins\n    Research\n    Tools\n",
    );
}
