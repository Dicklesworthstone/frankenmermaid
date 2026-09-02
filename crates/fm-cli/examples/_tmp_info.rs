fn main() {
    for (name, src) in [("info", "info\n"), ("treemap", "treemap\ntitle T\n\"A\": 10\n"), ("radar", "radar-beta\ntitle R\naxis a, b, c\ncurve x{1,2,3}\n")] {
        let ir = fm_parser::parse(src).ir;
        let layout = fm_layout::layout_diagram(&ir);
        let svg = fm_render_svg::render_svg(&ir);
        let vb = svg.find("viewBox=\"").map(|i| {
            let s = i + 9;
            svg[s..].find('"').map_or(String::new(), |e| svg[s..s + e].to_string())
        });
        println!(
            "{name:9} layout {}x{} nodes={} | svg viewBox={:?} bytes={} has_text={}",
            layout.bounds.width, layout.bounds.height, ir.nodes.len(), vb, svg.len(), svg.contains("<text")
        );
    }
}
