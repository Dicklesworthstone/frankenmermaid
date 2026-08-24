fn main() {
    for (tag, src) in [
        ("named", "xychart-beta\n  title \"X\"\n  x-axis [jan, feb, mar]\n  bar \"Revenue\" [10, 20, 30]\n"),
        ("unnamed", "xychart-beta\n  title \"X\"\n  x-axis [jan, feb]\n  bar [1, 2]\n"),
        ("no categories", "xychart-beta\n  title \"X\"\n  bar [7, 8]\n"),
    ] {
        let ir = fm_parser::parse(src).ir;
        for (label, spans) in [("streaming", false), ("element", true)] {
            let cfg = fm_render_svg::SvgRenderConfig { include_source_spans: spans, ..Default::default() };
            let svg = fm_render_svg::render_svg_with_config(&ir, &cfg);
            let titles: Vec<&str> = svg.match_indices("<title>").map(|(i, _)| {
                let rest = &svg[i + 7..];
                &rest[..rest.find("</title>").unwrap_or(0)]
            }).collect();
            println!("{tag:<14} {label:<10} titles={titles:?}");
        }
    }
}
