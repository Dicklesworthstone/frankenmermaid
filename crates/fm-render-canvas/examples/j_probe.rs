fn main() {
    let src = "journey\n  title J\n  section Go\n    TaskOne: 5: Alice, Bob\n";
    let ir = fm_parser::parse(src).ir;
    for (tag, cfg) in [
        ("default", fm_render_svg::SvgRenderConfig::default()),
        ("spans", fm_render_svg::SvgRenderConfig { include_source_spans: true, ..Default::default() }),
        ("no-embed", fm_render_svg::SvgRenderConfig { embed_theme_css: false, ..Default::default() }),
    ] {
        let svg = fm_render_svg::render_svg_with_config(&ir, &cfg);
        let titles: Vec<&str> = svg.match_indices("<title>").map(|(i, _)| {
            let r = &svg[i + 7..];
            &r[..r.find("</title>").unwrap_or(0)]
        }).collect();
        println!("{tag:<10} {titles:?}");
    }
}
