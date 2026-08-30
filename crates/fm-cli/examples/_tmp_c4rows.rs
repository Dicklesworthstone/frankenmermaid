//! Scratch diagnostic for bd-c23yq: print C4 cluster/node geometry in layout space.
//!
//! Kept as an example rather than deleted after use — RULE 1 forbids deleting files without express
//! permission, and that includes ones this session created.

fn main() {
    let src = "C4Context\n    title Sys\n    System_Boundary(sb, \"Core\") {\n        \
               Person(a, \"Alice\", \"A user\")\n    }\n";
    let ir = fm_parser::parse(src).ir;
    let layout = fm_layout::layout_diagram(&ir);
    println!("diagram bounds {:?}", layout.bounds);
    for c in &layout.clusters {
        println!(
            "cluster idx={} bounds x={} y={} w={} h={}",
            c.cluster_index, c.bounds.x, c.bounds.y, c.bounds.width, c.bounds.height
        );
    }
    for n in &layout.nodes {
        println!(
            "node {} bounds x={} y={} w={} h={}",
            n.node_id, n.bounds.x, n.bounds.y, n.bounds.width, n.bounds.height
        );
    }
    for (i, cluster) in ir.clusters.iter().enumerate() {
        println!(
            "ir cluster {i}: id={:?} title={:?}",
            cluster.id, cluster.title
        );
    }
}
