use fm_parser::parse;

#[test]
fn test_html_label() {
    let input = "digraph G { a [label=<b>Alpha</b>]; }";
    let parsed = parse(input, fm_core::MermaidParseMode::Lenient, fm_core::MermaidComplexity::default());
    assert_eq!(parsed.ir.nodes.len(), 1);
    assert_eq!(parsed.ir.labels.len(), 1);
}
