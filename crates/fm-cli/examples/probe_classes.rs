use fm_parser::parse;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let shape = a.get(1).map(String::as_str).unwrap_or("class");
    let input = match shape {
        "class" => "classDiagram\n  class C0 { +int f0 +m0() }\n  class C1 { +int f1 +m1() }\n  C0 <|-- C1".to_string(),
        "er" => "erDiagram\n  E0 ||--o{ E1 : has\n  E1 ||--o{ E2 : has".to_string(),
        _ => "flowchart LR\n  N0[Node 0]\n  N0-->N1".to_string(),
    };
    let pf = parse(&input);
    for (i, n) in pf.ir.nodes.iter().enumerate().take(4) {
        eprintln!("node[{i}] id={:?} classes={:?}", n.id, n.classes);
    }
}
