fn main() {
    let named = "quadrantChart\n  title Q\n  quadrant-1 TopRight\n  quadrant-2 TopLeft\n  \
                 quadrant-3 BottomLeft\n  quadrant-4 BottomRight\n  \
                 HiHi: [0.9, 0.9]\n  LoHi: [0.1, 0.9]\n  LoLo: [0.1, 0.1]\n  HiLo: [0.9, 0.1]\n";
    let unnamed = "quadrantChart\n  title Q\n  Alpha: [0.9, 0.25]\n";
    for (tag, src) in [("named", named), ("unnamed", unnamed)] {
        let ir = fm_parser::parse(src).ir;
        for (label, embed) in [("streaming", true), ("element", false)] {
            let cfg = fm_render_svg::SvgRenderConfig { embed_theme_css: embed, ..Default::default() };
            let svg = fm_render_svg::render_svg_with_config(&ir, &cfg);
            let names: Vec<&str> = svg.match_indices("class=\"fm-quadrant-point\"").filter_map(|(i, _)| {
                let rest = &svg[i..];
                let close = rest.find('>')?;
                let after = &rest[close + 1..];
                let t = after.strip_prefix("<title>")?;
                Some(&t[..t.find("</title>")?])
            }).collect();
            println!("{tag:<9} {label:<10} {names:?}");
        }
    }
}
