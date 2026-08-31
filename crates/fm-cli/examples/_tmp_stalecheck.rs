fn main() {
    // bd-lfrlx: does the bracket label actually become the LABEL, or just avoid vanishing?
    let c = fm_parser::parse("classDiagram\n  class A[\"Pretty Label\"]\n  A : +x\n").ir;
    for n in &c.nodes {
        let label = n.label.map(|l| c.labels[l.0].clone());
        println!(
            "bd-lfrlx node id={:?} label={:?} classes={:?}",
            n.id, label, n.classes
        );
    }
    // control: the same class without a bracket label
    let c2 = fm_parser::parse("classDiagram\n  class A\n  A : +x\n").ir;
    println!(
        "bd-lfrlx control nodes={:?}",
        c2.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>()
    );

    // bd-vc1zp: is the click a phantom TASK, and does the href survive?
    let g = fm_parser::parse("gantt\n  dateFormat YYYY-MM-DD\n  section S\n  Task A :a1, 2024-01-01, 30d\n  click a1 href \"https://example.com\"\n").ir;
    for n in &g.nodes {
        println!(
            "bd-vc1zp node id={:?} interaction={:?}",
            n.id,
            n.interaction.is_some()
        );
    }
    let g2 = fm_parser::parse(
        "gantt\n  dateFormat YYYY-MM-DD\n  section S\n  Task A :a1, 2024-01-01, 30d\n",
    )
    .ir;
    println!(
        "bd-vc1zp control (no click) nodes={:?}",
        g2.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>()
    );

    // bd-am6a2: note text preserved for both cases?
    for (name, src) in [
        ("lower", "note right of A: hi"),
        ("Upper", "Note right of A: hi"),
    ] {
        let ir = fm_parser::parse(&format!(
            "sequenceDiagram\n  participant A\n  participant B\n  {src}\n  A->>B: m\n"
        ))
        .ir;
        let notes: Vec<String> = ir
            .sequence_meta
            .as_ref()
            .map(|m| m.notes.iter().map(|n| format!("{:?}", n.text)).collect())
            .unwrap_or_default();
        println!("bd-am6a2 {name}: notes={notes:?}");
    }
}
