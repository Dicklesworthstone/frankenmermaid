//! Byte-identity probe: hash rendered SVG (encodes all layout coords) across shapes.
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
fn gen_in(shape: &str, n: usize) -> String {
    let mut l: Vec<String> = Vec::new();
    match shape {
        "mindmap" => {
            l.push("mindmap".into());
            l.push("  root".into());
            for i in 0..n {
                l.push(format!("    child{i}"));
            }
        }
        "wide" => {
            let w = ((n as f64).sqrt() as usize).max(4);
            let ranks = n / w;
            l.push("flowchart TB".into());
            for i in 0..n {
                l.push(format!("  N{i}[N{i}]"));
            }
            for r in 0..ranks.saturating_sub(1) {
                for c in 0..w {
                    let s = r * w + c;
                    let d = (r + 1) * w + ((c + 3) % w);
                    if s < n && d < n {
                        l.push(format!("  N{s}-->N{d}"));
                    }
                }
            }
        }
        "styled" => {
            l.push("flowchart LR".into());
            for i in 0..n {
                l.push(format!("  N{i}[Node {i}]:::myCustomNodeClass"));
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  N{i}-->N{}", i + 1));
            }
        }
        "subg" => {
            l.push("flowchart TB".into());
            l.push("  subgraph Big".into());
            for i in 0..n {
                l.push(format!("    N{i}"));
            }
            l.push("  end".into());
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  N{i}-->N{}", i + 1));
            }
        }
        "seq" => {
            let p = ((n as f64).sqrt() as usize).max(3);
            l.push("sequenceDiagram".into());
            for i in 0..p {
                l.push(format!("  participant P{i}"));
            }
            for i in 0..n {
                let a = i % p;
                let b = (i + 1) % p;
                l.push(format!("  P{a}->>P{b}: message {i}"));
            }
        }
        "state" => {
            l.push("stateDiagram-v2".into());
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  S{i} --> S{}", i + 1));
            }
        }
        _ => {
            l.push("flowchart LR".into());
            for i in 0..n {
                l.push(format!("  N{i}[Node {i}]"));
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  N{i}-->N{}", i + 1));
            }
        }
    }
    l.join("\n")
}
fn main() {
    let cfg = SvgRenderConfig::default();
    for shape in ["mindmap", "wide", "styled", "subg", "seq", "state", "flow"] {
        for n in [50usize, 200, 400] {
            let input = gen_in(shape, n);
            let pf = parse(&input);
            let layout = fm_layout::layout_diagram(&pf.ir);
            let svg = render_svg_with_layout(&pf.ir, &layout, &cfg);
            let mut h = DefaultHasher::new();
            svg.hash(&mut h);
            println!(
                "{shape:8} n={n:4} len={:7} hash={:016x}",
                svg.len(),
                h.finish()
            );
        }
    }
}
