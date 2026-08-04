use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
fn fnv(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
fn go(name: &str, input: &str) {
    let pf = parse(input);
    let l = fm_layout::layout_diagram(&pf.ir);
    let svg = render_svg_with_layout(&pf.ir, &l, &SvgRenderConfig::default());
    eprintln!("{name}: svg_fnv={:016x} len={}", fnv(&svg), svg.len());
}
fn main() {
    let mut s = String::from("mindmap\n  root\n");
    for i in 0..300 {
        s.push_str(&format!("    c{i}\n"));
    }
    go("mm_star", &s);
    // also a flowchart to ensure the index path change is byte-identical there too
    let mut f = String::from("flowchart LR\n");
    for i in 0..300 {
        f.push_str(&format!("  N{i}\n"));
    }
    for i in 0..299 {
        f.push_str(&format!("  N{i}-->N{}\n", i + 1));
    }
    go("flow", &f);
    let mut w = String::from("flowchart TB\n");
    for i in 0..400 {
        w.push_str(&format!("  N{i}\n"));
    }
    for i in 0..380 {
        w.push_str(&format!("  N{i}-->N{}\n", i + 20));
    }
    go("wide", &w);
}
