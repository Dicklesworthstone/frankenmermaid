use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
fn fnv(s: &str) -> u64 { let mut h=0xcbf29ce484222325u64; for b in s.bytes() { h^=b as u64; h=h.wrapping_mul(0x100000001b3); } h }
fn go(name:&str, input:&str) {
    let pf = parse(input);
    let layout = fm_layout::layout_diagram(&pf.ir);
    let svg = render_svg_with_layout(&pf.ir, &layout, &SvgRenderConfig::default());
    // also dump cluster/subgraph members to catch IR-level differences
    let cm: Vec<_> = pf.ir.clusters.iter().map(|c| c.members.clone()).collect();
    let sm: Vec<_> = pf.ir.graph.subgraphs.iter().map(|s| s.members.clone()).collect();
    eprintln!("{name}: svg_fnv={:016x} svg_len={} clusters={:?} subgraphs={:?}", fnv(&svg), svg.len(), cm, sm);
}
fn main() {
    go("one_big", "flowchart TB\n  subgraph Big\n    A\n    B\n    C\n  end\n  A-->B-->C");
    go("multi", "flowchart LR\n  subgraph S1\n    A\n    B\n  end\n  subgraph S2\n    C\n    D\n  end\n  A-->C\n  B-->D");
    go("nested", "flowchart TB\n  subgraph Outer\n    subgraph Inner\n      A\n      B\n    end\n    C\n  end\n  A-->B-->C");
    go("dup_node", "flowchart TB\n  subgraph S\n    A\n    A\n    B\n  end\n  A-->B");
    go("edge_in_sub", "flowchart TB\n  subgraph S\n    A-->B\n    B-->C\n  end");
}
