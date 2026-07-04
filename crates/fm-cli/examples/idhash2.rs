use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
mod g { include!("gen_shared.rs"); }
fn main() {
    let cfg = SvgRenderConfig::default();
    for shape in ["gantt","wide_flow","flow","sankey","state","class","er","pie","git"] {
        for n in [50usize, 200, 800] {
            let input = if shape=="wide_flow" {
                let w=((n as f64).sqrt() as usize).max(4); let ranks=n/w; let mut l=vec!["flowchart TB".to_string()];
                for i in 0..n { l.push(format!("  N{i}[N{i}]")); }
                for r in 0..ranks.saturating_sub(1) { for c in 0..w { let s=r*w+c; let d=(r+1)*w+((c+3)%w); if s<n&&d<n { l.push(format!("  N{s}-->N{d}")); } } }
                l.join("\n")
            } else if shape=="flow" {
                let mut l=vec!["flowchart LR".to_string()]; for i in 0..n { l.push(format!("  N{i}")); } for i in 0..n.saturating_sub(1){l.push(format!("  N{i}-->N{}",i+1));} l.join("\n")
            } else { g::gen_input(shape, n) };
            let pf = parse(&input);
            let layout = fm_layout::layout_diagram(&pf.ir);
            let svg = render_svg_with_layout(&pf.ir, &layout, &cfg);
            let mut h = DefaultHasher::new(); svg.hash(&mut h);
            println!("{shape:10} n={n:4} len={:8} hash={:016x}", svg.len(), h.finish());
        }
    }
}
