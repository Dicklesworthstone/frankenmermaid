fn main() {
    let t = fm_parser::parse("treemap-beta\n\"Root\"\n  \"A\": 10\n  \"B\": 20\n").ir;
    println!(
        "treemap meta rows = {:?}",
        t.treemap_meta.as_ref().map(|m| m.nodes.len())
    );
    let r = fm_parser::parse("radar-beta\n  title R\n  axis a, b, c\n  curve x{1,2,3}\n").ir;
    println!(
        "radar axes={:?} curves={:?}",
        r.radar_meta.as_ref().map(|m| m.axes.len()),
        r.radar_meta.as_ref().map(|m| m.curves.len())
    );
    let svg_t = fm_render_svg::render_svg(&t);
    let svg_r = fm_render_svg::render_svg(&r);
    println!("treemap svg has 'Root' = {}", svg_t.contains("Root"));
    println!("radar svg has axis 'a' = {}", svg_r.contains(">a<"));
}
