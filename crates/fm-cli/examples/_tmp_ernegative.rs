fn main() {
    for src in [
        "erDiagram\n  ONE ||--|| TWO : r\n",
        "erDiagram\n  MANY ||--|| OTHER : r\n",
        "erDiagram\n  TOTAL ||--|| PART : r\n",
        "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n",
        "erDiagram\n  A one to many B : r\n",
        "erDiagram\n  A many to one B : r\n",
        "erDiagram\n  A many(1) to many(0) B : r\n",
        "erDiagram\n  STOCK to WAREHOUSE : r\n",
    ] {
        let ir = fm_parser::parse(src).ir;
        let ids: Vec<&str> = ir.nodes.iter().map(|n| n.id.as_str()).collect();
        let n = ir
            .edges
            .first()
            .and_then(|e| e.er_notation())
            .map(str::to_string);
        println!(
            "{:<46} nodes={ids:?} edges={} notation={n:?}",
            src.lines().nth(1).unwrap().trim(),
            ir.edges.len()
        );
    }
}
