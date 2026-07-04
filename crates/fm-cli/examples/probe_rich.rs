use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Instant;
fn rich_class(n: usize, members: usize) -> String {
    let mut l = vec!["classDiagram".to_string()];
    for i in 0..n {
        l.push(format!("  class C{i} {{"));
        for m in 0..members {
            l.push(format!("    +int field{m}"));
            l.push(format!("    +method{m}(int a) bool"));
        }
        l.push("  }".into());
    }
    for i in 0..n.saturating_sub(1) {
        l.push(format!("  C{i} <|-- C{}", i + 1));
    }
    l.join("\n")
}
fn main() {
    let cfg = SvgRenderConfig::default();
    let mode = std::env::args().nth(1).unwrap_or("time".into());
    for (n, mem) in [(40usize, 20usize), (80, 20), (40, 40)] {
        let input = rich_class(n, mem);
        let pf = parse(&input);
        let layout = fm_layout::layout_diagram(&pf.ir);
        if mode == "hash" {
            let svg = render_svg_with_layout(&pf.ir, &layout, &cfg);
            let mut h = DefaultHasher::new();
            svg.hash(&mut h);
            println!("n={n} mem={mem} len={} hash={:016x}", svg.len(), h.finish());
        } else {
            let mut best = u128::MAX;
            for _ in 0..2000 {
                let t = Instant::now();
                let v = render_svg_with_layout(&pf.ir, &layout, &cfg);
                let e = t.elapsed().as_nanos();
                if e < best {
                    best = e;
                }
                std::hint::black_box(&v);
            }
            println!("n={n} mem={mem} render_min={best}ns");
        }
    }
}
