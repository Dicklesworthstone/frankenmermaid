//! Scratch diagnostic for bd-pdz8z / bd-mqmx2: how does a DOT document carrying mermaid
//! directives get routed, and does the graph survive?
//!
//! Kept rather than deleted after use — RULE 1 covers files this session created.

fn main() {
    let cases = [
        ("plain digraph", "digraph G {\n  a -> b\n}\n"),
        (
            "trailing deck",
            "digraph G {\n  a -> b\n}\n%%{deck: {slides: [{id: 's', nodes: ['a']}]}}%%\n",
        ),
        (
            "trailing init",
            "digraph G {\n  a -> b\n}\n%%{init: {'theme':'dark'}}%%\n",
        ),
        (
            "LEADING init",
            "%%{init: {'theme':'dark'}}%%\ndigraph G {\n  a -> b\n}\n",
        ),
        (
            "LEADING deck",
            "%%{deck: {slides: [{id: 's', nodes: ['a']}]}}%%\ndigraph G {\n  a -> b\n}\n",
        ),
        (
            "LEADING and trailing",
            "%%{init: {'theme':'dark'}}%%\ndigraph G {\n  a -> b\n}\n%%{deck: {slides: []}}%%\n",
        ),
        // Controls: mermaid documents that must NOT become DOT.
        ("flowchart brace node", "graph\n  A{decision} --> B\n"),
        (
            "flowchart brace node + leading init",
            "%%{init: {'theme':'dark'}}%%\ngraph\n  A{decision} --> B\n",
        ),
        (
            "classDiagram + leading init",
            "%%{init: {'theme':'dark'}}%%\nclassDiagram\n  class A { }\n",
        ),
    ];
    for (name, src) in cases {
        let detected = fm_parser::detect_type_with_confidence(src);
        let parsed = fm_parser::parse(src);
        println!(
            "{name:<36} method={:<14?} nodes={} edges={} deck={} theme={:?}",
            detected.method,
            parsed.ir.nodes.len(),
            parsed.ir.edges.len(),
            parsed.ir.deck.is_some(),
            parsed
                .ir
                .meta
                .theme_overrides
                .theme
                .as_deref()
                .unwrap_or("-"),
        );
    }
}
