//! Scratch profiling harness (not committed).
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn gen_input(shape: &str, n: usize) -> String {
    let mut l = Vec::new();
    match shape {
        "class" => {
            l.push("classDiagram".into());
            for i in 0..n {
                l.push(format!("  class C{i} {{"));
                l.push(format!("    +int field{i}"));
                l.push(format!("    +method{i}() bool"));
                l.push("  }".into());
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  C{i} <|-- C{}", i + 1));
            }
        }
        "classbad" => {
            l.push("classDiagram".into());
            for i in 0..n {
                l.push(format!("  class C{i} {{ +int field{i} +method{i}() }}"));
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  C{i} <|-- C{}", i + 1));
            }
        }
        "classcard" => {
            l.push("classDiagram".into());
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  C{i} \"1\" --> \"*\" C{} : rel{i}", i + 1));
            }
        }
        "er" => {
            l.push("erDiagram".into());
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  E{i} ||--o{{ E{} : has", i + 1));
            }
        }
        "gantt" => {
            l.push("gantt".into());
            l.push("  title Project".into());
            l.push("  dateFormat YYYY-MM-DD".into());
            let secs = ((n as f64).sqrt() as usize).max(2);
            for s in 0..secs {
                l.push(format!("  section Section {s}"));
                let per = n / secs;
                for t in 0..per {
                    let idx = s * per + t;
                    if t == 0 {
                        l.push(format!(
                            "  Task {idx} :t{idx}, 2024-01-01, {}d",
                            3 + (idx % 20)
                        ));
                    } else {
                        l.push(format!(
                            "  Task {idx} :t{idx}, after t{}, {}d",
                            idx - 1,
                            2 + (idx % 15)
                        ));
                    }
                }
            }
        }
        "state" => {
            l.push("stateDiagram-v2".into());
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  S{i} --> S{}", i + 1));
            }
        }
        "sankey" => {
            l.push("sankey-beta".into());
            for i in 0..n.saturating_sub(1) {
                l.push(format!("N{i},N{},{}", i + 1, 5 + (i % 20)));
            }
        }
        "pie" => {
            l.push("pie".into());
            for i in 0..n {
                l.push(format!("  \"Slice {i}\" : {}", 1 + (i % 50)));
            }
        }
        "journey" => {
            l.push("journey".into());
            l.push("  title My Journey".into());
            let secs = ((n as f64).sqrt() as usize).max(2);
            for s in 0..secs {
                l.push(format!("  section Section {s}"));
                let per = n / secs;
                for t in 0..per {
                    let idx = s * per + t;
                    l.push(format!(
                        "    Task {idx}: {}: Actor{}",
                        1 + (idx % 5),
                        idx % 3
                    ));
                }
            }
        }
        "quadrant" => {
            l.push("quadrantChart".into());
            l.push("  title Q".into());
            l.push("  x-axis Low --> High".into());
            l.push("  y-axis Low --> High".into());
            for i in 0..n {
                l.push(format!(
                    "  Point {i}: [{:.2}, {:.2}]",
                    (i % 100) as f32 / 100.0,
                    ((i * 7) % 100) as f32 / 100.0
                ));
            }
        }
        "requirement" => {
            l.push("requirementDiagram".into());
            for i in 0..n {
                l.push(format!("  requirement R{i} {{"));
                l.push(format!("    id: {i}"));
                l.push("    text: the requirement text".into());
                l.push("    risk: high".into());
                l.push("    verifymethod: test".into());
                l.push("  }".into());
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  R{i} - satisfies -> R{}", i + 1));
            }
        }
        "timeline" => {
            l.push("timeline".into());
            l.push("  title Timeline".into());
            let secs = ((n as f64).sqrt() as usize).max(2);
            for s in 0..secs {
                l.push(format!("  section Period {s}"));
                let per = n / secs;
                for t in 0..per {
                    let idx = s * per + t;
                    l.push(format!("    {} : Event {idx}", 2000 + idx));
                }
            }
        }
        "kanban" => {
            l.push("kanban".into());
            let cols = ((n as f64).sqrt() as usize).max(2);
            for c in 0..cols {
                l.push(format!("  col{c}[Column {c}]"));
                let per = n / cols;
                for t in 0..per {
                    let idx = c * per + t;
                    l.push(format!("    task{idx}[Task {idx}]"));
                }
            }
        }
        "block" => {
            l.push("block-beta".into());
            l.push("columns 3".into());
            for i in 0..n {
                if i % 10 == 0 {
                    l.push("  space".into());
                } else {
                    l.push(format!("  B{i}[\"Block {i}\"]"));
                }
            }
        }
        "mindmap" => {
            l.push("mindmap".into());
            l.push("  root".into());
            for i in 0..n {
                l.push(format!("    child{i}"));
            }
        }
        "git" => {
            // gitGraph: sqrt(N) branches, N commits distributed (stresses per-node cluster find)
            let b = ((n as f64).sqrt() as usize).max(2);
            l.push("gitGraph".into());
            l.push("  commit".into());
            for i in 0..b {
                l.push(format!("  branch b{i}"));
                l.push(format!("  checkout b{i}"));
                let per = n / b;
                for _ in 0..per {
                    l.push("  commit".into());
                }
            }
        }
        "subg" => {
            // one big subgraph with N nodes (stresses O(subgraph^2) membership dedup)
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
            // sequence: sqrt(n) participants, n messages between them
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
        "styled" => {
            l.push("flowchart LR".into());
            for i in 0..n {
                l.push(format!("  N{i}[Node {i}]:::myCustomNodeClass"));
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  N{i}-->N{}", i + 1));
            }
        }
        "flowo" => {
            l.push("flowchart LR".into());
            for i in 0..n {
                l.push(format!("  N{i}[Node {i}]"));
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  N{i}--oN{}", i + 1));
            }
        }
        "flowround" => {
            l.push("flowchart LR".into());
            for i in 0..n {
                l.push(format!("  N{i}(Node {i})"));
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  N{i}-->N{}", i + 1));
            }
        }
        "flowstad" => {
            l.push("flowchart LR".into());
            for i in 0..n {
                l.push(format!("  N{i}([Node {i}])"));
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  N{i}-->N{}", i + 1));
            }
        }
        "flowx" => {
            l.push("flowchart LR".into());
            for i in 0..n {
                l.push(format!("  N{i}[Node {i}]"));
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  N{i}--xN{}", i + 1));
            }
        }
        "styled3" => {
            l.push("flowchart LR".into());
            for i in 0..n {
                l.push(format!("  N{i}[Node {i}]:::serviceNodeStyle:::regionUsEastPrimary:::observabilityDashboard"));
            }
            for i in 0..n.saturating_sub(1) {
                l.push(format!("  N{i}-->N{}", i + 1));
            }
        }
        "wide" => {
            // grid: W nodes per rank across R ranks, cross-edges to next rank (max crossings)
            let w = ((n as f64).sqrt() as usize).max(4);
            let ranks = n / w;
            l.push("flowchart TB".into());
            for i in 0..n {
                l.push(format!("  N{i}[N{i}]"));
            }
            for r in 0..ranks.saturating_sub(1) {
                for c in 0..w {
                    let src = r * w + c;
                    let dst = (r + 1) * w + ((c + 3) % w);
                    if src < n && dst < n {
                        l.push(format!("  N{src}-->N{dst}"));
                    }
                }
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
    let a: Vec<String> = std::env::args().collect();
    let shape = a.get(1).map(String::as_str).unwrap_or("class");
    let size: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let iters: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let phase = a.get(4).map(String::as_str).unwrap_or("render");
    let input = gen_input(shape, size);
    let pf = parse(&input);
    eprintln!(
        "shape={shape} size={size} iters={iters} phase={phase} bytes={}",
        input.len()
    );
    let cfg = SvgRenderConfig::default();
    let mut acc: usize = 0;
    let layout = fm_layout::layout_diagram(&pf.ir);
    let mut s: Vec<u64> = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = std::time::Instant::now();
        match phase {
            "render" => {
                let v = render_svg_with_layout(&pf.ir, &layout, &cfg);
                acc = acc.wrapping_add(v.len());
                std::hint::black_box(&v);
            }
            "layout" => {
                let x = fm_layout::layout_diagram(&pf.ir);
                acc = acc.wrapping_add(x.nodes.len());
                std::hint::black_box(&x);
            }
            "parse" => {
                let p = parse(&input);
                acc = acc.wrapping_add(p.ir.nodes.len());
                std::hint::black_box(&p);
            }
            "sugiyama" => {
                let x = fm_layout::layout_diagram_traced_with_algorithm(
                    &pf.ir,
                    fm_layout::LayoutAlgorithm::Sugiyama,
                );
                acc = acc.wrapping_add(x.layout.nodes.len());
                std::hint::black_box(&x);
            }
            _ => {
                let p = parse(&input);
                let x = fm_layout::layout_diagram(&p.ir);
                let v = render_svg_with_layout(&p.ir, &x, &cfg);
                acc = acc.wrapping_add(v.len());
                std::hint::black_box(&v);
            }
        }
        s.push(t0.elapsed().as_nanos() as u64);
    }
    s.sort_unstable();
    eprintln!("min={}ns median={}ns acc={acc}", s[0], s[s.len() / 2]);
}
