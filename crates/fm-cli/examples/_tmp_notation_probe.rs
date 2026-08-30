// Diff our ER relation grammar against the PINNED incumbent's own lexer rules
// (mermaid 11.15.0, sha256 70137e77..., ER lexer rules 41-76).
fn main() {
    // (left cardinality, relType, right cardinality) exactly as the incumbent lexes them.
    let cases: &[&str] = &[
        // symbolic — the forms we already model
        "||--o{",
        "}o--o{",
        // relType variants the incumbent accepts but our table lacks
        "||.-o{",
        "||-.o{",
        // word-form cardinalities (incumbent rules 41-56, 63-66)
        "one or more to zero or more",
        "one or zero to one or many",
        "zero or many to many(1)",
        "1+ to 0+",
        "only one to many",
        "one to many(0)",
        "many optionally to one",
        // MD_PARENT (rule 67): `u` immediately before . - or |
        "u--||",
    ];
    for c in cases {
        let src = format!("erDiagram\n  A {c} B : r\n");
        let ir = fm_parser::parse(&src).ir;
        let names: Vec<&str> = ir.nodes.iter().map(|n| n.id.as_str()).collect();
        let notation = ir.edges.first().and_then(|e| e.er_notation());
        println!(
            "{c:32} nodes={:?} edges={} notation={notation:?}",
            names,
            ir.edges.len()
        );
    }
}
