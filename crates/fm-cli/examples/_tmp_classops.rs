fn main() {
    for op in ["--", "()--", "--()", "()..", "<|--"] {
        let src = format!("classDiagram\n  A {op} B\n");
        let parsed = fm_parser::parse(&src);
        let ir = &parsed.ir;
        let ids: Vec<&str> = ir.nodes.iter().map(|n| n.id.as_str()).collect();
        let arrow = ir.edges.first().map(|e| format!("{:?}", e.arrow));
        let svg = fm_render_svg::render_svg(ir);
        let markers: Vec<&str> = svg
            .match_indices("marker-")
            .map(|(i, _)| &svg[i..(i + 28).min(svg.len())])
            .collect();
        println!("{op:6} ids={ids:?} arrow={arrow:?} warns={}", parsed.ir.diagnostics.len());
        println!("        markers={markers:?}");
    }
}
