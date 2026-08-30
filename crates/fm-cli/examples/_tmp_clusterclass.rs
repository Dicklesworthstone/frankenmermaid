fn main() {
    let src = "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  classDef hot fill:#ff0000\n  class one hot\n";
    let ir = fm_parser::parse(src).ir;
    let layout = fm_layout::layout_diagram(&ir);
    for spans in [false, true] {
        let cfg = fm_render_svg::SvgRenderConfig {
            include_source_spans: spans,
            ..Default::default()
        };
        let svg = fm_render_svg::render_svg_with_layout(&ir, &layout, &cfg);
        println!(
            "spans={spans} marker={} cluster_class={:?}",
            svg.contains("fm-cluster-user-hot"),
            svg.split("class=\"fm-cluster")
                .nth(1)
                .map(|s| &s[..s.find('"').unwrap_or(0).min(40)])
        );
    }
}
