fn main() {
    let src = "gantt\n  title T\n  section S\n  Alpha :a1, 2024-01-01, 30d\n  click a1 href \"https://example.com\" \"tip\"\n";
    let ir = fm_parser::parse(src).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let regions = fm_render_canvas::hit_regions(&ir, &layout);
    println!("REGIONS {}", regions.len());
    for r in &regions {
        println!(
            "  id={:?} href={:?} tooltip={:?} bounds={:?}",
            r.node_id, r.href, r.tooltip, r.bounds
        );
    }
    println!("layout nodes: {}", layout.nodes.len());
    for n in &layout.nodes {
        println!("  node_index={} bounds={:?}", n.node_index, n.bounds);
    }
}
