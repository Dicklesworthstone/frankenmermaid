fn main() {
    let src = "erDiagram\n    USER {\n        string name \"the display name\"\n    }\n";
    let ir = fm_parser::parse(src).ir;
    for (name, embed) in [
        ("streaming (embed_theme_css=true)", true),
        ("Element path (embed_theme_css=false)", false),
    ] {
        let cfg = fm_render_svg::SvgRenderConfig {
            embed_theme_css: embed,
            ..Default::default()
        };
        let svg = fm_render_svg::render_svg_with_config(&ir, &cfg);
        let has = svg.contains("the display name");
        let row: Vec<&str> = svg
            .match_indices("fm-er-attribute\">")
            .map(|(i, _)| {
                let rest = &svg[i + 17..];
                &rest[..rest.find('<').unwrap_or(0)]
            })
            .collect();
        println!("{name:38} comment_drawn={has:5} rows={row:?}");
    }
}
