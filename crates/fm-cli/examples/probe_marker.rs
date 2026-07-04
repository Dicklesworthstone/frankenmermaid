use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
fn fnv(s:&str)->u64{let mut h=0xcbf29ce484222325u64;for b in s.bytes(){h^=b as u64;h=h.wrapping_mul(0x100000001b3);}h}
fn go(name:&str,input:&str){let pf=parse(input);let l=fm_layout::layout_diagram(&pf.ir);let svg=render_svg_with_layout(&pf.ir,&l,&SvgRenderConfig::default());eprintln!("{name}: fnv={:016x} len={}",fnv(&svg),svg.len());}
fn main(){
    go("arrows","flowchart LR\n A-->B\n B---C\n C-.->D\n D==>E\n E--x F\n F--o G");
    go("bidir","flowchart LR\n A<-->B\n A<==>C\n A o--o D\n A x--x E");
    go("mixed_small","flowchart TB\n A[Start]-->B{Dec}\n B-->|yes|C\n B-->|no|D");
    go("pie","pie title P\n \"a\": 3\n \"b\": 2");
    go("seq","sequenceDiagram\n Alice->>Bob: hi\n Bob-->>Alice: yo");
    go("er","erDiagram\n A ||--o{ B : has\n B }o--|| C : in");
    go("state","stateDiagram-v2\n [*]-->A\n A-->B\n B-->[*]");
    go("class","classDiagram\n A <|-- B\n A *-- C\n A o-- D");
    { let mut i=String::from("flowchart LR\n"); for k in 0..60 { i.push_str(&format!("  N{k}-->N{}\n",k+1)); } go("big60",&i); }
}
