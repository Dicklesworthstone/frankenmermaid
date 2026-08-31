fn main() {
    let cases: &[(&str, &str)] = &[
        (
            "create/destroy",
            "sequenceDiagram\n  participant A\n  participant B\n  create participant C\n  A->>C: hi\n  destroy C\n  A->>B: m\n",
        ),
        (
            "box",
            "sequenceDiagram\n  box Group\n    participant A\n  end\n  participant B\n  A->>B: m\n",
        ),
        (
            "critical",
            "sequenceDiagram\n  participant A\n  participant B\n  critical Check\n    A->>B: m\n  end\n",
        ),
        (
            "break",
            "sequenceDiagram\n  participant A\n  participant B\n  break when fail\n    A->>B: m\n  end\n",
        ),
        (
            "rect",
            "sequenceDiagram\n  participant A\n  participant B\n  rect rgb(0,255,0)\n    A->>B: m\n  end\n",
        ),
        (
            "baseline",
            "sequenceDiagram\n  participant A\n  participant B\n  A->>B: m\n",
        ),
    ];
    for (name, src) in cases {
        let parsed = fm_parser::parse(src);
        let ir = &parsed.ir;
        let ids: Vec<&str> = ir.nodes.iter().map(|n| n.id.as_str()).collect();
        println!(
            "{name:<15} nodes={ids:?} edges={} diags={}",
            ir.edges.len(),
            ir.diagnostics.len()
        );
    }
}
